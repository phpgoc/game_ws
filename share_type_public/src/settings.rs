use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::fmt;

/// Game settings trait - allows game-specific settings to be handled in common
pub trait GameSettings: Send + Sync + fmt::Debug {
    /// Serialize full settings (with min/max/current) for client display
    fn to_full_json(&self) -> Value;
    
    /// Serialize only current values for SETTING responses
    fn to_current_json(&self) -> Value;
    
    /// Update current values from a request
    fn update_from_json(&mut self, data: &Value) -> Result<(), String>;
    
    /// Clone as Box for storage
    fn clone_box(&self) -> Box<dyn GameSettings>;
}

/// Implement Clone for Box<dyn GameSettings>
impl Clone for Box<dyn GameSettings> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Generic parameter with min/max/current values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameParam {
    pub current: i32,
    pub min: i32,
    pub max: i32,
}

impl GameParam {
    pub fn new(current: i32, min: i32, max: i32) -> Self {
        Self { current, min, max }
    }
    
    /// Validate and update current value
    pub fn set_current(&mut self, value: i32) -> Result<(), String> {
        if value < self.min || value > self.max {
            return Err(format!("Value {} out of range [{}, {}]", value, self.min, self.max));
        }
        self.current = value;
        Ok(())
    }
}
