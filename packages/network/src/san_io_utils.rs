#[derive(Debug, Default)]
pub struct TasksSwitcher<const TASKS: usize> {
    stack: Vec<u8>,
}

impl<const TASKS: usize> TasksSwitcher<TASKS> {
    pub fn push_last<T: Into<u8>>(&mut self, task: T) {
        let value: u8 = task.into();
        debug_assert!((value as usize) < TASKS, "value {value} should < {TASKS}");
        if self.stack.contains(&value) {
            return;
        }
        self.stack.push(value);
    }

    pub fn push_all(&mut self) {
        self.stack.clear();
        for i in (0..TASKS).rev() {
            self.push_last(i as u8);
        }
    }

    /// Returns the current index of the task group, if it's not finished. Otherwise, returns None.
    pub fn current(&mut self) -> Option<usize> {
        self.stack.last().map(|x| *x as usize)
    }

    /// Flag that the current task group is finished.
    pub fn process<R>(&mut self, res: Option<R>) -> Option<R> {
        if res.is_none() {
            self.stack.pop();
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::TasksSwitcher;

    #[test]
    fn loop_style() {
        let mut tasks = TasksSwitcher::<3>::default();
        tasks.push_all();

        assert_eq!(tasks.current(), Some(0));
        assert_eq!(tasks.process(Some(1)), Some(1));
        assert_eq!(tasks.current(), Some(0));

        assert_eq!(tasks.process::<u8>(None), None);
        assert_eq!(tasks.current(), Some(1));

        assert_eq!(tasks.process::<u8>(None), None);
        assert_eq!(tasks.current(), Some(2));

        assert_eq!(tasks.process::<u8>(None), None);
        assert_eq!(tasks.current(), None);
        assert_eq!(tasks.current(), None);
    }

    #[test]
    fn stack_style() {
        let mut tasks = TasksSwitcher::<3>::default();

        tasks.push_last(2);
        tasks.push_last(0);

        assert_eq!(tasks.current(), Some(0));
        assert_eq!(tasks.process(Some(1)), Some(1));
        assert_eq!(tasks.current(), Some(0));

        assert_eq!(tasks.process::<u8>(None), None);
        assert_eq!(tasks.current(), Some(2));

        assert_eq!(tasks.process::<u8>(None), None);
        assert_eq!(tasks.current(), None);
        assert_eq!(tasks.current(), None);
    }
}
