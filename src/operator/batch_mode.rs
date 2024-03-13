#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::block::BatchMode;
    use crate::config::RuntimeConfig;
    use crate::environment::StreamContext;
    use crate::test::FakeOperator;

    #[test]
    fn batch_mode_fixed() {
        let env = StreamContext::new(RuntimeConfig::local(4));
        let source = FakeOperator::<u8>::empty();
        let batch_mode = BatchMode::fixed(42);
        let stream = env.stream(source).batch_mode(batch_mode);
        assert_eq!(stream.block.batch_mode, batch_mode);
    }

    #[test]
    fn batch_mode_adaptive() {
        let env = StreamContext::new(RuntimeConfig::local(4));
        let source = FakeOperator::<u8>::empty();
        let batch_mode = BatchMode::adaptive(42, Duration::from_secs(42));
        let stream = env.stream(source).batch_mode(batch_mode);
        assert_eq!(stream.block.batch_mode, batch_mode);
    }

    #[test]
    fn batch_inherit_from_previous() {
        let env = StreamContext::new(RuntimeConfig::local(4));
        let source = FakeOperator::<u8>::empty();
        let batch_mode = BatchMode::adaptive(42, Duration::from_secs(42));
        let stream = env.stream(source).batch_mode(batch_mode).group_by(|_| 0);
        assert_eq!(stream.0.block.batch_mode, batch_mode);
    }
}
