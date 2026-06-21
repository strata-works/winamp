#[derive(Clone, PartialEq, Debug)]
pub enum StateValue {
    Bool(bool),
    Scalar(f32),
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
