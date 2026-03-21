//! Command system — command IDs with enable/disable states.
//!
//! Inspired by Borland Turbo Vision's command architecture.

/// Command identifier type.
pub type CommandId = u16;

/// Bitfield-based command set for enable/disable tracking.
#[derive(Debug, Clone)]
pub struct CommandSet {
    /// Bitfield storage — each bit represents one command ID.
    bits: Vec<u32>,
}

impl CommandSet {
    /// Create a new `CommandSet` with all commands enabled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bits: vec![0xFFFF_FFFF; 64], // 64 * 32 = 2048 commands
        }
    }

    /// Check if a command is enabled.
    #[must_use]
    pub fn is_enabled(&self, command: CommandId) -> bool {
        let idx = command as usize / 32;
        let bit = command as usize % 32;
        if idx < self.bits.len() {
            self.bits[idx] & (1 << bit) != 0
        } else {
            false
        }
    }

    /// Enable a command.
    pub fn enable(&mut self, command: CommandId) {
        let idx = command as usize / 32;
        let bit = command as usize % 32;
        if idx < self.bits.len() {
            self.bits[idx] |= 1 << bit;
        }
    }

    /// Disable a command.
    pub fn disable(&mut self, command: CommandId) {
        let idx = command as usize / 32;
        let bit = command as usize % 32;
        if idx < self.bits.len() {
            self.bits[idx] &= !(1 << bit);
        }
    }
}

impl Default for CommandSet {
    fn default() -> Self {
        Self::new()
    }
}

// --- Standard Command IDs ---

// Dialog commands (< 100)
pub const CM_VALID: CommandId = 0;
pub const CM_QUIT: CommandId = 1;
pub const CM_OK: CommandId = 10;
pub const CM_CANCEL: CommandId = 11;
pub const CM_YES: CommandId = 12;
pub const CM_NO: CommandId = 13;
pub const CM_CLOSE: CommandId = 25;

// System broadcast commands (50-69)
pub const CM_RECEIVED_FOCUS: CommandId = 50;
pub const CM_RELEASED_FOCUS: CommandId = 51;
pub const CM_COMMAND_SET_CHANGED: CommandId = 52;
pub const CM_SCROLL_CHANGED: CommandId = 60;

// File commands (100-119)
pub const CM_NEW: CommandId = 100;
pub const CM_OPEN: CommandId = 101;
pub const CM_SAVE: CommandId = 102;
pub const CM_SAVE_AS: CommandId = 103;

// Edit commands (120-139)
pub const CM_UNDO: CommandId = 120;
pub const CM_REDO: CommandId = 121;
pub const CM_CUT: CommandId = 122;
pub const CM_COPY: CommandId = 123;
pub const CM_PASTE: CommandId = 124;
pub const CM_SELECT_ALL: CommandId = 125;
pub const CM_FIND: CommandId = 126;
pub const CM_REPLACE: CommandId = 127;

// View commands (140-159)
pub const CM_ZOOM: CommandId = 140;
pub const CM_NEXT: CommandId = 141;
pub const CM_PREV: CommandId = 142;
pub const CM_TILE: CommandId = 143;
pub const CM_CASCADE: CommandId = 144;

/// Commands >= `INTERNAL_COMMAND_BASE` are internal view commands
/// and will NOT close modal dialogs.
pub const INTERNAL_COMMAND_BASE: CommandId = 1000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_set_default_all_enabled() {
        let set = CommandSet::new();
        assert!(set.is_enabled(CM_OK));
        assert!(set.is_enabled(CM_QUIT));
        assert!(set.is_enabled(CM_SAVE));
    }

    #[test]
    fn test_command_set_disable_enable() {
        let mut set = CommandSet::new();
        assert!(set.is_enabled(CM_SAVE));

        set.disable(CM_SAVE);
        assert!(!set.is_enabled(CM_SAVE));

        set.enable(CM_SAVE);
        assert!(set.is_enabled(CM_SAVE));
    }

    #[test]
    fn test_command_set_out_of_range() {
        let set = CommandSet::new();
        // Command beyond our 2048 range
        assert!(!set.is_enabled(5000));
    }

    #[test]
    fn test_internal_command_base() {
        assert!(CM_OK < INTERNAL_COMMAND_BASE);
        assert!(CM_SAVE < INTERNAL_COMMAND_BASE);
        // Internal commands should be >= 1000
        assert!(1500 >= INTERNAL_COMMAND_BASE);
    }
}
