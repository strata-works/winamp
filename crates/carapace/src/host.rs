use std::time::Duration;

use crate::state::StateValue;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Bool(bool),
    Str(String),
}

#[derive(Clone, Copy, Debug)]
pub struct ActionSpec {
    pub name: &'static str,
}

/// The host capability surface. The engine knows none of the concrete names.
pub trait Host {
    fn name(&self) -> &str;
    fn tick(&mut self, dt: Duration);
    fn get(&self, key: &str) -> Option<StateValue>;
    fn actions(&self) -> &[ActionSpec];
    fn invoke(&mut self, action: &str, args: &[Value]);
}
