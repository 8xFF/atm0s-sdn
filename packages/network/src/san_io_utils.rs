use num::traits::{FromPrimitive, ToPrimitive};

#[derive(Debug, Default)]
pub struct TasksSwitcher<T: ToPrimitive, const TASKS: usize> {
    stack: Vec<T>,
}

impl<T: Copy + Eq + FromPrimitive + ToPrimitive, const TASKS: usize> TasksSwitcher<T, TASKS> {
    pub fn push_last(&mut self, value: T) {
        let value_s: usize = value.to_usize().expect("Should convert to usize");
        debug_assert!(value_s < TASKS, "value {value_s} should < {TASKS}");
        if self.stack.contains(&value) {
            return;
        }
        self.stack.push(value);
    }

    pub fn push_all(&mut self) {
        self.stack.clear();
        for i in (0..TASKS).rev() {
            self.push_last(T::from_usize(i).expect("Should convert from usize"));
        }
    }

    /// Returns the current index of the task group, if it's not finished. Otherwise, returns None.
    pub fn current(&mut self) -> Option<T> {
        self.stack.last().map(|x| *x)
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
        let mut tasks = TasksSwitcher::<u8, 3>::default();
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
        let mut tasks = TasksSwitcher::<u8, 3>::default();

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
