// Capture name constants shared by all query packs
pub const DEF_FUNCTION: &str = "def.function";
pub const DEF_METHOD: &str = "def.method";
pub const DEF_TYPE: &str = "def.type";
pub const REF_CALL: &str = "ref.call";
pub const REF_CALL_RECEIVER: &str = "ref.call.receiver";
pub const IMPORT: &str = "import";

// Semantic pattern captures — prefix "pattern." (individual consts used as documentation)
#[allow(dead_code)]
pub const PATTERN_PREFIX: &str = "pattern.";
#[allow(dead_code)]
pub const PATTERN_ALLOC: &str = "pattern.alloc";
#[allow(dead_code)]
pub const PATTERN_LOCK: &str = "pattern.lock";
#[allow(dead_code)]
pub const PATTERN_AWAIT: &str = "pattern.await";
#[allow(dead_code)]
pub const PATTERN_BLOCK_ON: &str = "pattern.block_on";
#[allow(dead_code)]
pub const PATTERN_SPAWN: &str = "pattern.spawn";
#[allow(dead_code)]
pub const PATTERN_LOOP: &str = "pattern.loop";
#[allow(dead_code)]
pub const PATTERN_CHAN: &str = "pattern.chan";
#[allow(dead_code)]
pub const PATTERN_TIMER: &str = "pattern.timer";
