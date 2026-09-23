/// Undo/redo over whole-state snapshots, skipping pushes that change nothing.
///
/// The signature returns a **hash**, not a string. It used to build one: for a scene
/// that meant a `format!` allocation per element, collected into a `Vec<String>` and
/// joined — and `push` computed it *twice*, once for the incoming value and once for
/// the current top of stack. At 20k elements that was several megabytes of string
/// churn for the act of drawing one rectangle, which is why drawing got slower as a
/// board filled up. Hashing allocates nothing.
pub struct SnapshotHistory<T> {
    stack: Vec<T>,
    index: usize,
    signature: fn(&T) -> u64,
    /// The signature of `stack[index]`, so a push hashes only the incoming value.
    current: u64,
    limit: usize,
}

impl<T: Clone> SnapshotHistory<T> {
    pub fn new(initial: T, signature: fn(&T) -> u64, limit: usize) -> Self {
        let current = signature(&initial);
        Self {
            stack: vec![initial],
            index: 0,
            signature,
            current,
            limit,
        }
    }

    pub fn push(&mut self, value: T) {
        let next = (self.signature)(&value);
        if next == self.current {
            return;
        }
        self.stack.truncate(self.index + 1);
        self.stack.push(value);
        if self.stack.len() > self.limit {
            self.stack.remove(0);
        }
        self.index = self.stack.len() - 1;
        self.current = next;
    }

    pub fn can_undo(&self) -> bool {
        self.index > 0
    }

    pub fn can_redo(&self) -> bool {
        self.index < self.stack.len() - 1
    }

    pub fn undo(&mut self) -> Option<&T> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        self.current = (self.signature)(&self.stack[self.index]);
        Some(&self.stack[self.index])
    }

    pub fn redo(&mut self) -> Option<&T> {
        if self.index >= self.stack.len() - 1 {
            return None;
        }
        self.index += 1;
        self.current = (self.signature)(&self.stack[self.index]);
        Some(&self.stack[self.index])
    }

    /// The entry the history is on now.
    pub fn current(&self) -> &T {
        &self.stack[self.index]
    }

    /// Replaces the entry the history is on, without moving.
    ///
    /// Undo and redo re-stamp what they restore, so the scene they leave behind is not
    /// byte-for-byte the entry they landed on. Without writing it back, the next push —
    /// even one that changes nothing — would differ from the top by those stamps alone,
    /// be recorded as a step, and throw the redo stack away.
    pub fn replace_current(&mut self, value: T) {
        self.current = (self.signature)(&value);
        self.stack[self.index] = value;
    }

    pub fn reset(&mut self, value: T) {
        self.current = (self.signature)(&value);
        self.stack = vec![value];
        self.index = 0;
    }
}
