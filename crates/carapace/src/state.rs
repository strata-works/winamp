/// How host data crosses the engine/skin boundary. Returned by `Host::get`/
/// `Row::get`; consumed by `Engine::state`, list expansion, and the renderer's
/// `value_of`/`text_of`.
#[derive(Clone, PartialEq, Debug)]
pub enum StateValue {
    /// A flag; `true` behaves as `1.0` for fills.
    Bool(bool),
    /// A `0..1` level or index.
    Scalar(f32),
    /// Shared text, e.g. titles or list cells.
    Str(std::sync::Arc<str>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn str_value_constructs_clones_and_compares() {
        let a = StateValue::Str(Arc::from("hello"));
        let b = a.clone();
        assert_eq!(a, b);
        match b {
            StateValue::Str(s) => assert_eq!(&*s, "hello"),
            _ => panic!("expected Str"),
        }
    }
}
