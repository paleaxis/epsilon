//! Transaction-based undo/redo history.
//!
//! History stores reversible commands rather than document snapshots: each
//! command carries only the elements it inserts, removes or replaces. This
//! keeps memory proportional to the edited data instead of the scene size.
//! Transactions group the commands produced by a single user action, the undo
//! stack is bounded, and redo branches are discarded as soon as a new
//! transaction is committed after an undo.
//!
//! The layer is intentionally UI-independent. The app decides when an action is
//! finished; repeated drag updates can call [`History::commit_coalescing`] so a
//! whole drag occupies one history entry.

use std::fmt;

use crate::document::{Document, Element, ElementId};

/// Default number of transactions retained for undo.
pub const DEFAULT_HISTORY_LIMIT: usize = 100;

/// Error returned when a command cannot be applied or reverted because the
/// document does not match the state the command was recorded against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryError {
    /// A command references an element that is not in the document.
    MissingElement(ElementId),
    /// An element would be inserted twice, or already exists.
    DuplicateElement(ElementId),
    /// An insert position lies beyond the document's element vector.
    InvalidInsertIndex { index: usize, len: usize },
    /// Replacement snapshots do not describe the same set of element IDs.
    ReplaceMismatch,
}

impl fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingElement(id) => {
                write!(formatter, "element {} is not in the document", id.get())
            }
            Self::DuplicateElement(id) => {
                write!(formatter, "element {} is already present", id.get())
            }
            Self::InvalidInsertIndex { index, len } => write!(
                formatter,
                "insert index {index} is beyond the document length {len}"
            ),
            Self::ReplaceMismatch => {
                formatter.write_str("replacement elements do not describe the same IDs")
            }
        }
    }
}

impl std::error::Error for HistoryError {}

/// An element captured for removal, together with its vector position.
#[derive(Debug, Clone, PartialEq)]
pub struct RemovedElement {
    pub index: usize,
    pub element: Element,
}

/// A single reversible document edit.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Inserts elements at a vector position; the position preserves paint
    /// order because equal `z_order` values are broken by vector order.
    Insert {
        index: usize,
        elements: Vec<Element>,
    },
    /// Removes existing elements and remembers where to put them back.
    Remove { entries: Vec<RemovedElement> },
    /// Replaces elements in place, matched by ID.
    Replace {
        before: Vec<Element>,
        after: Vec<Element>,
    },
}

impl Command {
    pub fn insert(index: usize, elements: Vec<Element>) -> Self {
        Self::Insert { index, elements }
    }

    /// Captures the current state of `ids` so they can be removed and restored.
    pub fn remove(document: &Document, ids: &[ElementId]) -> Result<Self, HistoryError> {
        let mut entries = Vec::with_capacity(ids.len());
        let mut seen = Vec::with_capacity(ids.len());
        for id in ids {
            if seen.contains(id) {
                return Err(HistoryError::DuplicateElement(*id));
            }
            seen.push(*id);
            let index = document
                .elements
                .iter()
                .position(|element| element.id == *id)
                .ok_or(HistoryError::MissingElement(*id))?;
            entries.push(RemovedElement {
                index,
                element: document.elements[index].clone(),
            });
        }
        // Restoring in ascending original indices rebuilds the vector exactly.
        entries.sort_by_key(|entry| entry.index);
        Ok(Self::Remove { entries })
    }

    /// Records a before/after snapshot of the same elements.
    pub fn replace(before: Vec<Element>, after: Vec<Element>) -> Result<Self, HistoryError> {
        if before.len() != after.len() {
            return Err(HistoryError::ReplaceMismatch);
        }
        let mut ids = Vec::with_capacity(before.len());
        for element in &before {
            if ids.contains(&element.id) {
                return Err(HistoryError::DuplicateElement(element.id));
            }
            ids.push(element.id);
        }
        if after.iter().any(|element| !ids.contains(&element.id)) {
            return Err(HistoryError::ReplaceMismatch);
        }
        Ok(Self::Replace { before, after })
    }

    /// Applies this command to `document`.
    ///
    /// Commands validate before mutating, so a failed command leaves the
    /// document untouched.
    pub fn apply(&self, document: &mut Document) -> Result<(), HistoryError> {
        match self {
            Self::Insert { index, elements } => {
                if *index > document.elements.len() {
                    return Err(HistoryError::InvalidInsertIndex {
                        index: *index,
                        len: document.elements.len(),
                    });
                }
                let mut seen = Vec::with_capacity(elements.len());
                for element in elements {
                    if document
                        .elements
                        .iter()
                        .any(|current| current.id == element.id)
                        || seen.contains(&element.id)
                    {
                        return Err(HistoryError::DuplicateElement(element.id));
                    }
                    seen.push(element.id);
                }
                for (offset, element) in elements.iter().enumerate() {
                    document.elements.insert(index + offset, element.clone());
                }
                Ok(())
            }
            Self::Remove { entries } => {
                validate_removals(entries, document)?;
                for entry in entries {
                    remove_by_id(document, entry.element.id)?;
                }
                Ok(())
            }
            Self::Replace { after, .. } => {
                validate_replacements(after, document)?;
                for replacement in after {
                    let element = document
                        .elements
                        .iter_mut()
                        .find(|element| element.id == replacement.id)
                        .ok_or(HistoryError::MissingElement(replacement.id))?;
                    *element = replacement.clone();
                }
                Ok(())
            }
        }
    }

    /// Reverts this command, restoring the document to its previous state.
    pub fn revert(&self, document: &mut Document) -> Result<(), HistoryError> {
        match self {
            Self::Insert { elements, .. } => {
                validate_replacements(elements, document)?;
                for element in elements {
                    remove_by_id(document, element.id)?;
                }
                Ok(())
            }
            Self::Remove { entries } => {
                validate_insertions(entries, document)?;
                for entry in entries {
                    document.elements.insert(entry.index, entry.element.clone());
                }
                Ok(())
            }
            Self::Replace { before, .. } => {
                validate_replacements(before, document)?;
                for replacement in before {
                    let element = document
                        .elements
                        .iter_mut()
                        .find(|element| element.id == replacement.id)
                        .ok_or(HistoryError::MissingElement(replacement.id))?;
                    *element = replacement.clone();
                }
                Ok(())
            }
        }
    }
}

fn validate_removals(entries: &[RemovedElement], document: &Document) -> Result<(), HistoryError> {
    let mut seen = Vec::with_capacity(entries.len());
    for entry in entries {
        if seen.contains(&entry.element.id) {
            return Err(HistoryError::DuplicateElement(entry.element.id));
        }
        seen.push(entry.element.id);
        if !document
            .elements
            .iter()
            .any(|element| element.id == entry.element.id)
        {
            return Err(HistoryError::MissingElement(entry.element.id));
        }
    }
    Ok(())
}

fn validate_insertions(
    entries: &[RemovedElement],
    document: &Document,
) -> Result<(), HistoryError> {
    let mut seen = Vec::with_capacity(entries.len());
    for (offset, entry) in entries.iter().enumerate() {
        let len = document.elements.len() + offset;
        if entry.index > len {
            return Err(HistoryError::InvalidInsertIndex {
                index: entry.index,
                len,
            });
        }
        if document
            .elements
            .iter()
            .any(|element| element.id == entry.element.id)
            || seen.contains(&entry.element.id)
        {
            return Err(HistoryError::DuplicateElement(entry.element.id));
        }
        seen.push(entry.element.id);
    }
    Ok(())
}

fn validate_replacements(
    replacements: &[Element],
    document: &Document,
) -> Result<(), HistoryError> {
    let mut seen = Vec::with_capacity(replacements.len());
    for replacement in replacements {
        if seen.contains(&replacement.id) {
            return Err(HistoryError::DuplicateElement(replacement.id));
        }
        seen.push(replacement.id);
        if !document
            .elements
            .iter()
            .any(|element| element.id == replacement.id)
        {
            return Err(HistoryError::MissingElement(replacement.id));
        }
    }
    Ok(())
}

fn remove_by_id(document: &mut Document, id: ElementId) -> Result<Element, HistoryError> {
    let index = document
        .elements
        .iter()
        .position(|element| element.id == id)
        .ok_or(HistoryError::MissingElement(id))?;
    Ok(document.elements.remove(index))
}

/// A group of commands produced by one user action.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transaction {
    commands: Vec<Command>,
}

impl Transaction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(index: usize, elements: Vec<Element>) -> Self {
        Self {
            commands: vec![Command::insert(index, elements)],
        }
    }

    pub fn remove(document: &Document, ids: &[ElementId]) -> Result<Self, HistoryError> {
        Ok(Self {
            commands: vec![Command::remove(document, ids)?],
        })
    }

    pub fn replace(before: Vec<Element>, after: Vec<Element>) -> Result<Self, HistoryError> {
        Ok(Self {
            commands: vec![Command::replace(before, after)?],
        })
    }

    pub fn push(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// Applies every command, rolling back already-applied commands if one
    /// fails so the transaction stays all-or-nothing.
    pub fn apply(&self, document: &mut Document) -> Result<(), HistoryError> {
        for (index, command) in self.commands.iter().enumerate() {
            if let Err(error) = command.apply(document) {
                for applied in self.commands[..index].iter().rev() {
                    debug_assert!(
                        applied.revert(document).is_ok(),
                        "reverting an applied command must succeed"
                    );
                }
                return Err(error);
            }
        }
        Ok(())
    }

    /// Reverts every command in reverse order, rolling forward again if one
    /// fails.
    pub fn revert(&self, document: &mut Document) -> Result<(), HistoryError> {
        for (index, command) in self.commands.iter().enumerate().rev() {
            if let Err(error) = command.revert(document) {
                for applied in &self.commands[index + 1..] {
                    debug_assert!(
                        applied.apply(document).is_ok(),
                        "re-applying a reverted command must succeed"
                    );
                }
                return Err(error);
            }
        }
        Ok(())
    }

    /// Merges a repeated edit of the same elements into one entry.
    ///
    /// Only single replace commands are merged: drags produce a sequence of
    /// replacements for the same selection, and collapsing them makes one undo
    /// step per drag. The first before-state and the last after-state win.
    fn coalesce_with(&self, next: &Self) -> Option<Self> {
        let [Command::Replace { before, .. }] = self.commands.as_slice() else {
            return None;
        };
        let [
            Command::Replace {
                after: next_after, ..
            },
        ] = next.commands.as_slice()
        else {
            return None;
        };
        if before.len() != next_after.len() {
            return None;
        }

        let mut merged_after = Vec::with_capacity(before.len());
        for original in before {
            merged_after.push(
                next_after
                    .iter()
                    .find(|element| element.id == original.id)?
                    .clone(),
            );
        }
        Some(Self {
            commands: vec![Command::Replace {
                before: before.clone(),
                after: merged_after,
            }],
        })
    }
}

/// Bounded undo/redo stacks over a document.
#[derive(Debug, Clone)]
pub struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        Self::with_limit(DEFAULT_HISTORY_LIMIT)
    }

    /// Creates a history that retains at most `limit` transactions. A limit of
    /// zero applies edits without keeping them undoable.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
        }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Applies and records a new transaction, discarding any redo branch.
    pub fn commit(
        &mut self,
        document: &mut Document,
        transaction: Transaction,
    ) -> Result<(), HistoryError> {
        if transaction.is_empty() {
            return Ok(());
        }
        transaction.apply(document)?;
        self.redo.clear();
        self.push_undo(transaction);
        Ok(())
    }

    /// Applies and records a transaction, merging it into the previous entry
    /// when both replace the same elements. Use this for per-frame drag updates;
    /// separate user actions must go through [`History::commit`] so they do not
    /// merge into the previous entry.
    pub fn commit_coalescing(
        &mut self,
        document: &mut Document,
        transaction: Transaction,
    ) -> Result<(), HistoryError> {
        if transaction.is_empty() {
            return Ok(());
        }
        transaction.apply(document)?;
        self.redo.clear();
        if let Some(previous) = self.undo.last_mut()
            && let Some(merged) = previous.coalesce_with(&transaction)
        {
            *previous = merged;
            return Ok(());
        }
        self.push_undo(transaction);
        Ok(())
    }

    /// Reverts the most recent transaction. Returns `false` when there is
    /// nothing to undo.
    pub fn undo(&mut self, document: &mut Document) -> Result<bool, HistoryError> {
        let Some(transaction) = self.undo.pop() else {
            return Ok(false);
        };
        if let Err(error) = transaction.revert(document) {
            self.undo.push(transaction);
            return Err(error);
        }
        self.redo.push(transaction);
        Ok(true)
    }

    /// Re-applies the most recently undone transaction. Returns `false` when
    /// there is no redo branch.
    pub fn redo(&mut self, document: &mut Document) -> Result<bool, HistoryError> {
        let Some(transaction) = self.redo.pop() else {
            return Ok(false);
        };
        if let Err(error) = transaction.apply(document) {
            self.redo.push(transaction);
            return Err(error);
        }
        self.push_undo(transaction);
        Ok(true)
    }

    fn push_undo(&mut self, transaction: Transaction) {
        if self.limit == 0 {
            return;
        }
        self.undo.push(transaction);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::ElementKind;

    fn box_element(id: u64) -> Element {
        let mut element = Element::with_id(
            ElementId::new(id),
            ElementKind::Rectangle,
            id as f64,
            0.0,
            20.0,
            20.0,
        );
        element.z_order = id as i32;
        element
    }

    fn insert_element(history: &mut History, document: &mut Document, id: u64) {
        let transaction = Transaction::insert(document.elements.len(), vec![box_element(id)]);
        history
            .commit(document, transaction)
            .expect("insert element");
    }

    #[test]
    fn undo_and_redo_restore_document_exactly() {
        let mut document = Document::new();
        document.canvas.grid_visible = true;
        let mut history = History::new();
        let empty = document.clone();

        insert_element(&mut history, &mut document, 1);
        insert_element(&mut history, &mut document, 2);

        let before = document.elements[0].clone();
        let mut after = before.clone();
        after.x += 15.0;
        after.style.stroke_width = 6.0;
        after.version += 1;
        history
            .commit(
                &mut document,
                Transaction::replace(vec![before], vec![after]).expect("replace snapshot"),
            )
            .expect("replace element");

        let edited = document.clone();
        let removed_id = document.elements[1].id;
        let removal = Transaction::remove(&document, &[removed_id]).expect("capture removal");
        history
            .commit(&mut document, removal)
            .expect("remove element");
        let final_state = document.clone();
        assert_eq!(document.elements.len(), 1);

        assert!(history.undo(&mut document).expect("undo removal"));
        assert_eq!(document, edited);

        while history.undo(&mut document).expect("undo remaining edits") {}
        assert_eq!(document, empty);

        while history.redo(&mut document).expect("redo edits") {}
        assert_eq!(document, final_state);
    }

    #[test]
    fn one_transaction_groups_multiple_commands_into_one_undo_step() {
        let element = box_element(1);
        let mut moved = element.clone();
        moved.x = 40.0;
        let mut transaction = Transaction::insert(0, vec![element.clone()]);
        transaction.push(Command::replace(vec![element], vec![moved]).expect("replace snapshot"));

        let mut document = Document::new();
        let mut history = History::new();
        history
            .commit(&mut document, transaction)
            .expect("commit transaction");

        assert_eq!(document.elements.len(), 1);
        assert_eq!(document.elements[0].x, 40.0);
        assert_eq!(history.undo_len(), 1);

        assert!(history.undo(&mut document).expect("undo transaction"));
        assert!(document.elements.is_empty());

        assert!(history.redo(&mut document).expect("redo transaction"));
        assert_eq!(document.elements[0].x, 40.0);
    }

    #[test]
    fn new_edit_after_undo_discards_redo_branch() {
        let mut document = Document::new();
        let mut history = History::new();

        insert_element(&mut history, &mut document, 1);
        insert_element(&mut history, &mut document, 2);
        assert!(history.undo(&mut document).expect("undo second insert"));
        assert!(history.can_redo());

        insert_element(&mut history, &mut document, 3);

        assert!(!history.can_redo());
        assert!(!history.redo(&mut document).expect("redo discarded"));
        let ids: Vec<u64> = document
            .elements
            .iter()
            .map(|element| element.id.get())
            .collect();
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn undo_history_is_bounded_and_drops_oldest_entries() {
        let mut document = Document::new();
        let mut history = History::with_limit(2);

        for id in 1..=4 {
            insert_element(&mut history, &mut document, id);
        }

        assert_eq!(history.undo_len(), 2);
        assert!(history.undo(&mut document).expect("undo newest"));
        assert!(history.undo(&mut document).expect("undo second newest"));
        assert!(!history.undo(&mut document).expect("no entry remains"));
        let ids: Vec<u64> = document
            .elements
            .iter()
            .map(|element| element.id.get())
            .collect();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn zero_limit_applies_edits_without_recording_history() {
        let mut document = Document::new();
        let mut history = History::with_limit(0);

        insert_element(&mut history, &mut document, 1);

        assert_eq!(document.elements.len(), 1);
        assert!(!history.can_undo());
        assert!(!history.undo(&mut document).expect("nothing to undo"));
    }

    #[test]
    fn coalescing_collapses_drag_updates_into_one_entry() {
        let original = box_element(1);
        let mut document = Document::new();
        let mut history = History::new();
        history
            .commit(
                &mut document,
                Transaction::insert(0, vec![original.clone()]),
            )
            .expect("insert element");

        for x in [25.0, 50.0, 75.0] {
            let before = document.elements[0].clone();
            let mut after = before.clone();
            after.x = x;
            history
                .commit_coalescing(
                    &mut document,
                    Transaction::replace(vec![before], vec![after]).expect("replace snapshot"),
                )
                .expect("commit drag update");
        }

        // One entry for the insert and one for the whole drag.
        assert_eq!(history.undo_len(), 2);
        assert_eq!(document.elements[0].x, 75.0);
        assert!(history.undo(&mut document).expect("undo coalesced drag"));
        assert_eq!(document.elements[0].x, original.x);
    }

    #[test]
    fn coalescing_does_not_merge_edits_to_different_elements() {
        let mut document = Document::new();
        let mut history = History::new();
        history
            .commit(
                &mut document,
                Transaction::insert(0, vec![box_element(1), box_element(2)]),
            )
            .expect("insert elements");

        for position in 0..2 {
            let before = document.elements[position].clone();
            let mut after = before.clone();
            after.style.opacity = 0.5;
            history
                .commit_coalescing(
                    &mut document,
                    Transaction::replace(vec![before], vec![after]).expect("replace snapshot"),
                )
                .expect("commit update");
        }

        // One insert plus one entry per distinct element edit.
        assert_eq!(history.undo_len(), 3);
    }

    #[test]
    fn removing_multiple_elements_restores_vector_order_on_undo() {
        let elements = vec![
            box_element(1),
            box_element(2),
            box_element(3),
            box_element(4),
        ];
        let mut document = Document::new();
        let mut history = History::new();
        history
            .commit(&mut document, Transaction::insert(0, elements.clone()))
            .expect("insert elements");
        let snapshot = document.clone();

        let removed = vec![elements[0].id, elements[2].id];
        let transaction = Transaction::remove(&document, &removed).expect("capture removal");
        history
            .commit(&mut document, transaction)
            .expect("remove elements");
        let remaining: Vec<u64> = document
            .elements
            .iter()
            .map(|element| element.id.get())
            .collect();
        assert_eq!(remaining, vec![2, 4]);

        assert!(history.undo(&mut document).expect("undo removal"));
        assert_eq!(document, snapshot);
    }

    #[test]
    fn failed_commit_leaves_document_and_history_unchanged() {
        let mut document = Document::new();
        let mut history = History::new();
        insert_element(&mut history, &mut document, 1);
        let snapshot = document.clone();

        let missing = box_element(99);
        let mut replacement = missing.clone();
        replacement.x = 100.0;
        let transaction =
            Transaction::replace(vec![missing.clone()], vec![replacement]).expect("snapshot");
        let error = history
            .commit(&mut document, transaction)
            .expect_err("replacement target is missing");

        assert_eq!(error, HistoryError::MissingElement(missing.id));
        assert_eq!(document, snapshot);
        assert_eq!(history.undo_len(), 1);
    }
}
