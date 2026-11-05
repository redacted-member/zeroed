//! The undo journal.
//!
//! Every mutating command opens a step, records the changes it makes and
//! commits. A change stores what the document held *before* it, so undoing is
//! a matter of putting that back. Reverting a change returns the change that
//! would undo the revert, which is what the redo stack is built from, so the
//! two directions share one code path.
//!
//! Entity changes are per entity, not document snapshots. Layer and sheet
//! lists are short enough to keep whole.

use std::collections::VecDeque;

use super::entity::Entity;
use super::job::{JobSetup, Side};
use super::layer::{Layer, Sheet};
use crate::cam::toolpath::Toolpath;
use crate::model::{Component, Level};

/// How many steps the journal keeps. A typical default undo depth.
pub const UNDO_LIMIT: usize = 200;

/// One reversible edit.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// An entity's whole state. `before` is `None` when the entity did not
    /// exist. Where it sat in the z-order is restored by [`Change::Order`],
    /// which every command that touches the order records.
    Entity {
        /// Which entity.
        id: u64,
        /// What it was, or `None` when it did not exist.
        before: Option<Box<Entity>>,
    },
    /// The z-order of the top level entities.
    Order(Vec<u64>),
    /// The layer list and which layer was active.
    Layers(Vec<Layer>, u64),
    /// The sheet list and which sheet was active.
    Sheets(Vec<Sheet>, u64),
    /// The selection, in selection order.
    Selection(Vec<u64>),
    /// The job setup.
    Job(Box<JobSetup>),
    /// The project notes.
    Notes(String),
    /// The whole 3D assembly (small: parts refer to vectors by id).
    Assembly(Box<crate::assembly::Assembly>),
    /// The toolpath tiling settings.
    Tiling(Box<crate::cam::tiling::Tiling>),
    /// The distortions that can still be edited.
    Distortions(Vec<super::Distortion>),
    /// The Nest Parts marks of the objects.
    NestMarks(std::collections::BTreeMap<u64, super::nest_cmd::NestMark>),
    /// The document variables.
    Variables {
        /// Every variable's value.
        values: std::collections::BTreeMap<String, f64>,
        /// The formulas of the variables defined by one.
        formulas: std::collections::BTreeMap<String, String>,
    },
    /// The id counters. Counters only move forward, so undoing a create
    /// does not hand its id out again.
    Counters {
        /// Next entity id.
        entity: u64,
        /// Next layer id.
        layer: u64,
        /// Next sheet id.
        sheet: u64,
        /// Next toolpath id.
        toolpath: u64,
        /// Next model component id.
        component: u64,
    },
    /// One model component's whole state.
    Component {
        /// Which component.
        id: u64,
        /// What it was, or `None` when it did not exist.
        before: Option<Box<Component>>,
    },
    /// The order of the component list, bottom first.
    ComponentOrder(Vec<u64>),
    /// The model's levels, bottom first, the active one and the next id.
    Levels {
        /// The levels.
        levels: Vec<Level>,
        /// The level new components go on.
        active: u64,
        /// Next level id; it never goes back.
        next: u64,
    },
    /// One toolpath's whole state. `before` is `None` when it did not exist.
    Toolpath {
        /// Which toolpath.
        id: u64,
        /// What it was, or `None` when it did not exist.
        before: Option<Box<Toolpath>>,
    },
    /// The order of the toolpath list.
    ToolpathOrder(Vec<u64>),
    /// The toolpath groups.
    ToolpathGroups(Vec<super::ToolpathGroup>),
    /// Exchanges the live content with the other side's stored content of a
    /// double sided job. An operation rather than a snapshot, so reverting
    /// it does it again.
    SwapSides {
        /// Also turns the active side over, which is what switching sides
        /// does; Swap Sides leaves the active side where it is.
        toggle: bool,
        /// Other side content to put in place before swapping.
        install: Option<Box<super::sides::SideState>>,
        /// Takes the other side content out after swapping, which undoes
        /// the first switch to a side that had no content yet.
        remove: bool,
    },
}

/// One command's worth of changes, with the label the menus show.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// Label shown next to Undo and Redo, for instance "Offset".
    pub label: String,
    /// Changes in the order they were made. Reverted back to front.
    pub changes: Vec<Change>,
    /// The side of a double sided job the live content must be on for
    /// the changes to be reverted: where the step ended, for an undo step,
    /// and where the undo left the document, for a redo step. Switching
    /// sides is a view change outside the journal, so Undo and Redo turn
    /// to this side first. `None` for a step that does not care.
    pub side: Option<Side>,
}

impl Step {
    /// An empty step with the given label.
    pub fn new(label: impl Into<String>) -> Self {
        Step {
            label: label.into(),
            changes: Vec::new(),
            side: None,
        }
    }

    /// True when the command changed nothing.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// The undo and redo stacks plus the step being recorded.
#[derive(Debug, Clone, PartialEq)]
pub struct Journal {
    undo: VecDeque<Step>,
    redo: Vec<Step>,
    recording: Option<Step>,
    limit: usize,
    /// An open undo group: its label, if one was given, and how many steps
    /// have been pushed since it opened.
    group: Option<(Option<String>, usize)>,
    /// Steps committed so far, so a caller can tell whether a command made
    /// one.
    committed: u64,
}

impl Default for Journal {
    fn default() -> Self {
        Journal {
            undo: VecDeque::new(),
            redo: Vec::new(),
            recording: None,
            limit: UNDO_LIMIT,
            group: None,
            committed: 0,
        }
    }
}

impl Journal {
    /// An empty journal.
    pub fn new() -> Self {
        Journal::default()
    }

    /// Starts recording a step. A step already open is committed first, which
    /// only happens if a command forgot to close its own.
    pub fn begin(&mut self, label: impl Into<String>) {
        if self.recording.is_some() {
            self.commit();
        }
        self.recording = Some(Step::new(label));
    }

    /// True while a step is open.
    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Notes on the open step the side its changes were made on, which
    /// is the side Undo turns to before reverting them.
    pub fn set_side(&mut self, side: Side) {
        if let Some(step) = self.recording.as_mut() {
            step.side = Some(side);
        }
    }

    /// Records a change into the open step. Changes made outside a step are
    /// dropped, which is what read only commands want.
    pub fn record(&mut self, change: Change) {
        if let Some(step) = self.recording.as_mut() {
            step.changes.push(change);
        }
    }

    /// Closes the open step and pushes it, dropping the redo stack. An empty
    /// step is thrown away so that a command that changed nothing does not
    /// take an undo slot.
    pub fn commit(&mut self) -> bool {
        let Some(step) = self.recording.take() else {
            return false;
        };
        if step.is_empty() {
            return false;
        }
        self.undo.push_back(step);
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
        self.redo.clear();
        if let Some((_, steps)) = self.group.as_mut() {
            *steps += 1;
        }
        self.committed += 1;
        true
    }

    /// How many steps have been committed since the journal was made.
    pub fn committed(&self) -> u64 {
        self.committed
    }

    /// Opens an undo group: the steps committed until [`Journal::end_group`]
    /// become one step, called `label` or, without one, after the first of
    /// them. A group already open is closed first.
    pub fn begin_group(&mut self, label: Option<String>) {
        self.end_group();
        self.group = Some((label, 0));
    }

    /// True while an undo group is open.
    pub fn in_group(&self) -> bool {
        self.group.is_some()
    }

    /// Closes the open undo group, merging the steps committed inside it
    /// into one. Answers whether a merged step was pushed.
    pub fn end_group(&mut self) -> bool {
        let Some((label, steps)) = self.group.take() else {
            return false;
        };
        let count = steps.min(self.undo.len());
        if count == 0 || (count == 1 && label.is_none()) {
            return false;
        }
        let mut taken = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(step) = self.undo.pop_back() {
                taken.push(step);
            }
        }
        taken.reverse();
        let first = taken.first().map(|s| s.label.clone()).unwrap_or_default();
        let mut merged = Step::new(label.unwrap_or(first));
        for step in taken {
            merged.changes.extend(step.changes);
            merged.side = step.side.or(merged.side);
        }
        self.undo.push_back(merged);
        true
    }

    /// Throws the open step away without pushing it. The caller is expected
    /// to have put the document back itself, which is what happens when a
    /// command fails half way.
    pub fn abort(&mut self) -> Option<Step> {
        self.recording.take()
    }

    /// Takes the newest undo step off the stack.
    pub fn pop_undo(&mut self) -> Option<Step> {
        let step = self.undo.pop_back()?;
        // A step taken back inside an open group no longer counts in it.
        if let Some((_, steps)) = self.group.as_mut() {
            *steps = steps.saturating_sub(1);
        }
        Some(step)
    }

    /// Takes the newest undo step back off the stack and records into it
    /// again, so that follow-on changes land in the same step. Answers
    /// false when a step is already open or there is none.
    pub fn reopen(&mut self) -> bool {
        if self.recording.is_some() {
            return false;
        }
        match self.pop_undo() {
            Some(step) => {
                self.recording = Some(step);
                true
            }
            None => false,
        }
    }

    /// True when the newest undo step changed entity `id`.
    pub fn newest_touches(&self, id: u64) -> bool {
        self.undo.back().is_some_and(|step| {
            step.changes
                .iter()
                .any(|c| matches!(c, Change::Entity { id: e, .. } if *e == id))
        })
    }

    /// What entity `id` was before the newest undo step first changed it:
    /// `None` when that step left it alone, `Some(None)` when the step made it.
    pub fn newest_before(&self, id: u64) -> Option<Option<&Entity>> {
        self.undo.back()?.changes.iter().find_map(|c| match c {
            Change::Entity { id: e, before } if *e == id => Some(before.as_deref()),
            _ => None,
        })
    }

    /// Takes the newest redo step off the stack.
    pub fn pop_redo(&mut self) -> Option<Step> {
        self.redo.pop()
    }

    /// Pushes a step onto the undo stack without clearing the redo stack,
    /// which is what redoing does.
    pub fn push_undo(&mut self, step: Step) {
        self.undo.push_back(step);
        if let Some((_, steps)) = self.group.as_mut() {
            *steps += 1;
        }
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
    }

    /// Pushes a step onto the redo stack, which is what undoing does.
    pub fn push_redo(&mut self, step: Step) {
        self.redo.push(step);
    }

    /// Whether there is anything to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is anything to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Label of the step Undo would apply.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.back().map(|s| s.label.as_str())
    }

    /// Label of the step Redo would apply.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    /// How many steps are on the undo stack.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// How many steps the journal keeps.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Sets how many steps the journal keeps, at least one, dropping the
    /// oldest steps beyond it.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
    }

    /// Forgets everything, which `doc.new` and `doc.open` do.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.recording = None;
        self.group = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step_with(label: &str) -> Change {
        Change::Notes(label.to_string())
    }

    #[test]
    fn a_group_undoes_as_one_step() {
        let mut journal = Journal::new();
        journal.begin("Before");
        journal.record(step_with("zero"));
        journal.commit();
        journal.begin_group(None);
        journal.begin("Move");
        journal.record(step_with("a"));
        journal.commit();
        journal.begin("Empty");
        journal.commit();
        journal.begin("Transform Components");
        journal.record(step_with("b"));
        journal.commit();
        assert!(journal.end_group());
        assert_eq!(journal.undo_depth(), 2);
        assert_eq!(journal.undo_label(), Some("Move"));
        let merged = journal.pop_undo().map(|s| s.changes);
        assert_eq!(merged, Some(vec![step_with("a"), step_with("b")]));
    }

    #[test]
    fn a_step_taken_back_inside_a_group_leaves_the_steps_before_the_group_alone() {
        let mut journal = Journal::new();
        journal.begin("Before");
        journal.record(step_with("zero"));
        journal.commit();
        journal.begin_group(Some("Drag".to_string()));
        journal.begin("Transform Components");
        journal.record(step_with("a"));
        journal.commit();
        // A live drag replaces its last step: taken back, then made again.
        assert!(journal.pop_undo().is_some());
        journal.begin("Transform Components");
        journal.record(step_with("b"));
        journal.commit();
        assert!(journal.end_group());
        assert_eq!(journal.undo_depth(), 2, "the step before the group stays");
        assert_eq!(journal.undo_label(), Some("Drag"));
        assert_eq!(
            journal.pop_undo().map(|s| s.changes),
            Some(vec![step_with("b")])
        );
        assert_eq!(journal.undo_label(), Some("Before"));
    }

    #[test]
    fn a_labelled_group_renames_a_single_step_and_an_empty_one_pushes_nothing() {
        let mut journal = Journal::new();
        journal.begin_group(Some("Mirror".to_string()));
        assert!(!journal.end_group());
        assert!(!journal.can_undo());
        journal.begin_group(Some("Mirror".to_string()));
        journal.begin("Transform");
        journal.record(step_with("a"));
        journal.commit();
        assert!(journal.in_group());
        assert!(journal.end_group());
        assert!(!journal.in_group());
        assert_eq!(journal.undo_label(), Some("Mirror"));
        assert_eq!(journal.undo_depth(), 1);
    }

    #[test]
    fn an_empty_step_does_not_take_a_slot() {
        let mut journal = Journal::new();
        journal.begin("Move");
        assert!(!journal.commit());
        assert!(!journal.can_undo());
    }

    #[test]
    fn committing_a_step_makes_it_undoable() {
        let mut journal = Journal::new();
        journal.begin("Move");
        journal.record(step_with("before"));
        assert!(journal.commit());
        assert!(journal.can_undo());
        assert_eq!(journal.undo_label(), Some("Move"));
    }

    #[test]
    fn changes_recorded_outside_a_step_are_dropped() {
        let mut journal = Journal::new();
        journal.record(step_with("stray"));
        journal.begin("Move");
        assert!(!journal.commit());
    }

    #[test]
    fn committing_clears_the_redo_stack() {
        let mut journal = Journal::new();
        journal.push_redo(Step {
            label: "Old".to_string(),
            changes: vec![step_with("x")],
            side: None,
        });
        journal.begin("New");
        journal.record(step_with("y"));
        journal.commit();
        assert!(!journal.can_redo());
    }

    #[test]
    fn a_lower_limit_drops_the_oldest_steps() {
        let mut journal = Journal::new();
        for i in 0..10 {
            journal.begin(format!("Step {i}"));
            journal.record(Change::Notes(String::new()));
            journal.commit();
        }
        journal.set_limit(4);
        assert_eq!(journal.limit(), 4);
        assert_eq!(journal.undo_depth(), 4);
        assert_eq!(journal.undo_label(), Some("Step 9"));
        journal.begin("Step 10");
        journal.record(Change::Notes(String::new()));
        journal.commit();
        assert_eq!(journal.undo_depth(), 4);
        journal.set_limit(0);
        assert_eq!(journal.limit(), 1);
    }

    #[test]
    fn the_journal_caps_at_the_undo_limit() {
        let mut journal = Journal::new();
        for i in 0..UNDO_LIMIT + 20 {
            journal.begin(format!("Step {i}"));
            journal.record(step_with("x"));
            journal.commit();
        }
        assert_eq!(journal.undo_depth(), UNDO_LIMIT);
        assert_eq!(
            journal.undo_label(),
            Some(format!("Step {}", UNDO_LIMIT + 19).as_str())
        );
    }

    #[test]
    fn aborting_a_step_leaves_the_stacks_alone() {
        let mut journal = Journal::new();
        journal.begin("Move");
        journal.record(step_with("x"));
        let aborted = journal.abort().expect("a step was open");
        assert_eq!(aborted.label, "Move");
        assert!(!journal.can_undo());
        assert!(!journal.is_recording());
    }

    #[test]
    fn beginning_twice_commits_the_first_step() {
        let mut journal = Journal::new();
        journal.begin("First");
        journal.record(step_with("x"));
        journal.begin("Second");
        assert_eq!(journal.undo_label(), Some("First"));
    }

    #[test]
    fn clearing_forgets_both_stacks() {
        let mut journal = Journal::new();
        journal.begin("Move");
        journal.record(step_with("x"));
        journal.commit();
        journal.clear();
        assert!(!journal.can_undo());
        assert!(!journal.can_redo());
    }
}
