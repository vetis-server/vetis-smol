mod handler;

use crate::hook::HandlerHook;
use vetis::errors::{ConfigError, VetisError};

#[test]
fn test_handler_path_builder_validation() {
    let missing_handler = HandlerHook::builder().build();
    assert!(matches!(missing_handler, Err(VetisError::Config(ConfigError::Hook(_)))));
}
