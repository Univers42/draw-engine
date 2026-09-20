pub struct SnapshotHistory<T> {
    stack: Vec<T>,
    index: usize,
    signature: fn(&T) -> String,
    limit: usize,
}

impl<T: Clone> SnapshotHistory<T> {
    pub fn new(initial: T, signature: fn(&T) -> String, limit: usize) -> Self {
        Self {
            stack: vec![initial],
            index: 0,
            signature,
            limit,
        }
    }

    pub fn push(&mut self, value: T) {
        if (self.signature)(&value) == (self.signature)(&self.stack[self.index]) {
            return;
        }
        self.stack.truncate(self.index + 1);
        self.stack.push(value);
        if self.stack.len() > self.limit {
            self.stack.remove(0);
        }
        self.index = self.stack.len() - 1;
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
        Some(&self.stack[self.index])
    }

    pub fn redo(&mut self) -> Option<&T> {
        if self.index >= self.stack.len() - 1 {
            return None;
        }
        self.index += 1;
        Some(&self.stack[self.index])
    }

    pub fn reset(&mut self, value: T) {
        self.stack = vec![value];
        self.index = 0;
    }
}
