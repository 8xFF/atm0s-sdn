#[derive(Debug, Default)]
pub struct TasksSwitcher<const TASKS: usize> {
    current_index: usize,
}

impl<const TASKS: usize> TasksSwitcher<TASKS> {
    /// Returns the current index of the task group, if it's not finished. Otherwise, returns None.
    pub fn current(&mut self) -> Option<usize> {
        if self.current_index < TASKS {
            Some(self.current_index)
        } else {
            self.current_index = 0;
            None
        }
    }

    /// Flag that the current task group is finished.
    pub fn process<R>(&mut self, res: Option<R>) -> Option<R> {
        if res.is_none() {
            self.current_index += 1;
        }
        res
    }
}
