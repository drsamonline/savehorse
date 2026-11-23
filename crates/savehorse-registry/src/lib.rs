//! The operation manifest.
//!
//! Every operation in savehorse is declared exactly once, here. The CLI reads
//! this to parse arguments and print help; the desktop app will read it to
//! render forms; the docs are generated from it. One declaration, many
//! consumers — that is what stops eighty-eight operations from becoming
//! eighty-eight hand-written screens.
//!
//! An operation is either [`Status::Implemented`] (there is code behind it) or
//! [`Status::Planned`] (the capability exists in the original portfolio and is
//! queued for porting). Nothing is silently missing and nothing pretends to
//! work.

/// How much damage an operation can do if it goes wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Danger {
    /// Reads only. Safe to run anywhere.
    Safe,
    /// Writes, but reversibly, or behind a preview.
    Destructive,
    /// Cannot be undone: shredding, deletion, in-place patching.
    Irreversible,
}

impl Danger {
    /// A short label for help output.
    pub fn label(self) -> &'static str {
        match self {
            Danger::Safe => "safe",
            Danger::Destructive => "writes",
            Danger::Irreversible => "IRREVERSIBLE",
        }
    }
}

/// Whether there is working code behind the operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Implemented,
    Planned,
}

/// What kind of control this parameter wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// A single filesystem path.
    Path,
    /// One or more filesystem paths.
    Paths,
    /// Free text.
    Text,
    /// A regular expression or literal to match.
    Pattern,
    /// A whole number.
    Number,
    /// A byte size such as `10k` or `2m`.
    Size,
    /// A yes/no switch.
    Bool,
    /// One of a fixed set of words.
    Choice,
    /// A passphrase or key material; never echoed, never logged.
    Secret,
}

/// One parameter of one operation.
#[derive(Debug, Clone, Copy)]
pub struct Param {
    /// The flag name without leading dashes, e.g. `min-len`.
    pub name: &'static str,
    pub kind: ParamKind,
    /// A positional parameter must be supplied, in order.
    pub positional: bool,
    pub required: bool,
    pub help: &'static str,
    /// Allowed words for a `Choice` parameter. Empty for every other kind.
    pub options: &'static [&'static str],
}

/// One operation.
#[derive(Debug, Clone, Copy)]
pub struct Op {
    /// Stable identifier, `module.name`. Used by recipes and by the GUI.
    pub id: &'static str,
    pub module: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    pub danger: Danger,
    pub status: Status,
    pub params: &'static [Param],
}

/// One module: a group of operations that share a job.
#[derive(Debug, Clone, Copy)]
pub struct Module {
    pub name: &'static str,
    pub title: &'static str,
    pub job: &'static str,
    /// Which applications from the original portfolio this module replaces.
    pub sources: &'static str,
}

// ---------------------------------------------------------------------------
// Parameter shorthands
// ---------------------------------------------------------------------------

const P_PATH: Param = Param {
    name: "path",
    kind: ParamKind::Path,
    positional: true,
    required: true,
    help: "File or directory to work on",
    options: &[],
};

const P_PATHS: Param = Param {
    name: "paths",
    kind: ParamKind::Paths,
    positional: true,
    required: true,
    help: "One or more files or directories",
    options: &[],
};

const P_PATTERN: Param = Param {
    name: "pattern",
    kind: ParamKind::Pattern,
    positional: true,
    required: true,
    help: "Text or regular expression to match",
    options: &[],
};

const P_MIN_SIZE: Param = Param {
    name: "min-size",
    kind: ParamKind::Size,
    positional: false,
    required: false,
    help: "Skip anything smaller, e.g. 10k",
    options: &[],
};

const P_MAX_SIZE: Param = Param {
    name: "max-size",
    kind: ParamKind::Size,
    positional: false,
    required: false,
    help: "Skip anything larger, e.g. 2m",
    options: &[],
};

const P_EXT: Param = Param {
    name: "ext",
    kind: ParamKind::Text,
    positional: false,
    required: false,
    help: "Only this extension, e.g. txt",
    options: &[],
};

const P_DEPTH: Param = Param {
    name: "depth",
    kind: ParamKind::Number,
    positional: false,
    required: false,
    help: "Maximum directory depth",
    options: &[],
};

const P_GLOB: Param = Param {
    name: "glob",
    kind: ParamKind::Text,
    positional: false,
    required: false,
    help: "Only files whose name matches, e.g. *.rs",
    options: &[],
};

const P_INPUT: Param = Param {
    name: "input",
    kind: ParamKind::Path,
    positional: true,
    required: true,
    help: "File to read",
    options: &[],
};

const P_SOURCE: Param = Param {
    name: "source",
    kind: ParamKind::Path,
    positional: true,
    required: true,
    help: "Directory or file to read from",
    options: &[],
};

const P_IMAGE: Param = Param {
    name: "image",
    kind: ParamKind::Path,
    positional: true,
    required: true,
    help: "Disc image to read",
    options: &[],
};

const P_OUT: Param = Param {
    name: "out",
    kind: ParamKind::Path,
    positional: false,
    required: true,
    help: "Where to write the result",
    options: &[],
};

const P_TITLE: Param = Param {
    name: "title",
    kind: ParamKind::Text,
    positional: false,
    required: false,
    help: "Document title",
    options: &[],
};

const P_VOLUME_ID: Param = Param {
    name: "volume-id",
    kind: ParamKind::Text,
    positional: false,
    required: false,
    help: "Volume identifier stored in the image (default SAVEHORSE)",
    options: &[],
};

const P_OUT_OPT: Param = Param {
    name: "out",
    kind: ParamKind::Path,
    positional: false,
    required: false,
    help: "Where to write the result (a sensible default is used if omitted)",
    options: &[],
};

const P_ARCHIVE: Param = Param {
    name: "archive",
    kind: ParamKind::Path,
    positional: true,
    required: true,
    help: "Archive to read",
    options: &[],
};

const P_WIDTH: Param = Param {
    name: "width",
    kind: ParamKind::Number,
    positional: false,
    required: true,
    help: "Bounding box width in pixels",
    options: &[],
};

const P_HEIGHT: Param = Param {
    name: "height",
    kind: ParamKind::Number,
    positional: false,
    required: true,
    help: "Bounding box height in pixels",
    options: &[],
};

const P_ROTATE: Param = Param {
    name: "rotate",
    kind: ParamKind::Number,
    positional: false,
    required: false,
    help: "Rotate by 90, 180 or 270 degrees",
    options: &[],
};

const P_FLIP_H: Param = Param {
    name: "flip-h",
    kind: ParamKind::Bool,
    positional: false,
    required: false,
    help: "Mirror left to right",
    options: &[],
};

const P_FLIP_V: Param = Param {
    name: "flip-v",
    kind: ParamKind::Bool,
    positional: false,
    required: false,
    help: "Mirror top to bottom",
    options: &[],
};

const P_SIZE: Param = Param {
    name: "size",
    kind: ParamKind::Size,
    positional: false,
    required: false,
    help: "Longest edge in pixels, or a chunk size such as 10m",
    options: &[],
};

const P_FORMAT: Param = Param {
    name: "format",
    kind: ParamKind::Choice,
    positional: false,
    required: true,
    help: "Target image format",
    options: &["png", "jpg", "bmp", "gif", "ico"],
};

const P_TYPES: Param = Param {
    name: "types",
    kind: ParamKind::Text,
    positional: false,
    required: false,
    help: "Comma-separated formats to look for, e.g. png,jpg,pdf",
    options: &[],
};

const P_NUMBERS: Param = Param {
    name: "numbers",
    kind: ParamKind::Bool,
    positional: false,
    required: false,
    help: "Prefix each line with its number",
    options: &[],
};

const P_START: Param = Param {
    name: "start",
    kind: ParamKind::Number,
    positional: false,
    required: false,
    help: "First line to show (1-based)",
    options: &[],
};

const P_COUNT: Param = Param {
    name: "count",
    kind: ParamKind::Number,
    positional: false,
    required: false,
    help: "How many lines to show",
    options: &[],
};

const P_EOL_TO: Param = Param {
    name: "to",
    kind: ParamKind::Choice,
    positional: false,
    required: false,
    help: "Rewrite line endings to this style",
    options: &["crlf", "lf", "cr"],
};

const P_ENC_TO: Param = Param {
    name: "to",
    kind: ParamKind::Choice,
    positional: false,
    required: true,
    help: "Target encoding",
    options: &["utf8", "utf8-bom", "utf16le", "ascii"],
};

const P_IN_PLACE: Param = Param {
    name: "in-place",
    kind: ParamKind::Bool,
    positional: false,
    required: false,
    help: "Rewrite the file; without it this only reports what would change",
    options: &[],
};

const P_STRIP: Param = Param {
    name: "strip-trailing",
    kind: ParamKind::Bool,
    positional: false,
    required: false,
    help: "Remove spaces and tabs at the end of each line",
    options: &[],
};

const P_TAB: Param = Param {
    name: "tab-size",
    kind: ParamKind::Number,
    positional: false,
    required: false,
    help: "Expand tabs to this many spaces",
    options: &[],
};

const P_OFFSET: Param = Param {
    name: "offset",
    kind: ParamKind::Number,
    positional: false,
    required: true,
    help: "Byte offset in the file",
    options: &[],
};

const P_OFFSET_OPT: Param = Param {
    name: "offset",
    kind: ParamKind::Number,
    positional: false,
    required: false,
    help: "Byte offset to start at (default 0)",
    options: &[],
};

const P_LENGTH: Param = Param {
    name: "length",
    kind: ParamKind::Number,
    positional: false,
    required: false,
    help: "How many bytes to read (default 256)",
    options: &[],
};

const P_HEX: Param = Param {
    name: "hex",
    kind: ParamKind::Text,
    positional: false,
    required: true,
    help: "Bytes as hex, e.g. \"4D 5A\" or 4d5a",
    options: &[],
};

const P_BACKUP: Param = Param {
    name: "backup",
    kind: ParamKind::Bool,
    positional: false,
    required: false,
    help: "Keep a .bak copy before writing",
    options: &[],
};

const P_PATTERN_OPT: Param = Param {
    name: "pattern",
    kind: ParamKind::Text,
    positional: false,
    required: false,
    help: "Regular expression applied to the file name stem",
    options: &[],
};

const P_REPLACE: Param = Param {
    name: "replace",
    kind: ParamKind::Text,
    positional: false,
    required: false,
    help: "Replacement for what the pattern matches",
    options: &[],
};

const P_PREFIX: Param = Param {
    name: "prefix",
    kind: ParamKind::Text,
    positional: false,
    required: false,
    help: "Text to put before each name",
    options: &[],
};

const P_SUFFIX: Param = Param {
    name: "suffix",
    kind: ParamKind::Text,
    positional: false,
    required: false,
    help: "Text to put after each name",
    options: &[],
};

const P_CASE: Param = Param {
    name: "case",
    kind: ParamKind::Choice,
    positional: false,
    required: false,
    help: "Case transformation for the name",
    options: &["lower", "upper", "title", "snake"],
};

const P_EXT_LOWER: Param = Param {
    name: "ext-lower",
    kind: ParamKind::Bool,
    positional: false,
    required: false,
    help: "Lowercase the extension",
    options: &[],
};

const P_NUMBER: Param = Param {
    name: "number",
    kind: ParamKind::Bool,
    positional: false,
    required: false,
    help: "Append a sequential number",
    options: &[],
};

const P_STEP: Param = Param {
    name: "step",
    kind: ParamKind::Number,
    positional: false,
    required: false,
    help: "Increment between numbers (default 1)",
    options: &[],
};

const P_PAD: Param = Param {
    name: "pad",
    kind: ParamKind::Number,
    positional: false,
    required: false,
    help: "Zero-pad numbers to this width (default 3)",
    options: &[],
};

const P_PASSPHRASE: Param = Param {
    name: "passphrase",
    kind: ParamKind::Secret,
    positional: false,
    required: false,
    help: "Secret used to derive the key (or use --keyfile)",
    options: &[],
};

const P_KEYFILE: Param = Param {
    name: "keyfile",
    kind: ParamKind::Path,
    positional: false,
    required: false,
    help: "File whose bytes are mixed into the key",
    options: &[],
};

const P_SIGNATURES: Param = Param {
    name: "signatures",
    kind: ParamKind::Path,
    positional: false,
    required: false,
    help: "JSON array of { name, crc32 } to match against",
    options: &[],
};

const P_PATH_OPT: Param = Param {
    name: "path",
    kind: ParamKind::Path,
    positional: true,
    required: false,
    help: "Directory to audit (defaults to the system temporary directory)",
    options: &[],
};

const P_TOP: Param = Param {
    name: "top",
    kind: ParamKind::Number,
    positional: false,
    required: false,
    help: "How many of the largest files to list (default 10)",
    options: &[],
};

const P_JOURNAL: Param = Param {
    name: "journal",
    kind: ParamKind::Path,
    positional: false,
    required: false,
    help: "Where to write the undo journal (default: ./.savehorse-rename-<time>.json)",
    options: &[],
};

const P_JOURNAL_REQ: Param = Param {
    name: "journal",
    kind: ParamKind::Path,
    positional: true,
    required: true,
    help: "Undo journal written by a previous rename",
    options: &[],
};

const P_COLLAPSE: Param = Param {
    name: "collapse-blank",
    kind: ParamKind::Bool,
    positional: false,
    required: false,
    help: "Reduce runs of blank lines to one",
    options: &[],
};

const P_STORE: Param = Param {
    name: "store",
    kind: ParamKind::Bool,
    positional: false,
    required: false,
    help: "Store without compressing",
    options: &[],
};

const P_FILTER: Param = Param {
    name: "filter",
    kind: ParamKind::Text,
    positional: false,
    required: false,
    help: "Only extract names matching this pattern",
    options: &[],
};

const P_IGNORE_CASE: Param = Param {
    name: "ignore-case",
    kind: ParamKind::Bool,
    positional: false,
    required: false,
    help: "Match without regard to case",
    options: &[],
};

// ---------------------------------------------------------------------------
// sift — Text & Search
// ---------------------------------------------------------------------------

const SIFT_STRINGS: &[Param] = &[
    P_PATH,
    Param {
        name: "min-len",
        kind: ParamKind::Number,
        positional: false,
        required: false,
        help: "Shortest run of characters to report (default 4)",
        options: &[],
    },
    Param {
        name: "ascii",
        kind: ParamKind::Bool,
        positional: false,
        required: false,
        help: "ASCII only, skip the UTF-16 pass",
        options: &[],
    },
    Param {
        name: "unicode",
        kind: ParamKind::Bool,
        positional: false,
        required: false,
        help: "UTF-16 only, skip the ASCII pass",
        options: &[],
    },
];

const SIFT_SEARCH: &[Param] = &[
    P_PATTERN,
    P_PATHS,
    Param {
        name: "regex",
        kind: ParamKind::Bool,
        positional: false,
        required: false,
        help: "Treat the pattern as a regular expression",
        options: &[],
    },
    P_IGNORE_CASE,
    P_GLOB,
];

const SIFT_REPLACE: &[Param] = &[
    P_PATTERN,
    Param {
        name: "replacement",
        kind: ParamKind::Text,
        positional: true,
        required: true,
        help: "Text to substitute in place of each match",
        options: &[],
    },
    P_PATHS,
    P_IGNORE_CASE,
    Param {
        name: "backup",
        kind: ParamKind::Bool,
        positional: false,
        required: false,
        help: "Write a .bak copy before changing a file",
        options: &[],
    },
    P_GLOB,
];

const SIFT_XOR: &[Param] = &[
    P_PATTERN,
    P_PATH,
    Param {
        name: "max-key",
        kind: ParamKind::Number,
        positional: false,
        required: false,
        help: "Only try XOR keys up to this value (default 255)",
        options: &[],
    },
];

// ---------------------------------------------------------------------------
// scout — Filesystem
// ---------------------------------------------------------------------------

const SCOUT_CATALOG: &[Param] = &[
    P_PATH,
    Param {
        name: "format",
        kind: ParamKind::Choice,
        positional: false,
        required: false,
        help: "How to render the report",
        options: &["table", "json", "csv", "markdown", "html"],
    },
    Param {
        name: "out",
        kind: ParamKind::Path,
        positional: false,
        required: false,
        help: "Write the report here instead of stdout",
        options: &[],
    },
    P_MIN_SIZE,
    P_MAX_SIZE,
    P_EXT,
    P_DEPTH,
];

const SCOUT_DUPLICATES: &[Param] = &[P_PATH, P_MIN_SIZE, P_EXT, P_DEPTH];

const SCOUT_CLEAN: &[Param] = &[
    P_PATH,
    Param {
        name: "commit",
        kind: ParamKind::Bool,
        positional: false,
        required: false,
        help: "Actually delete; without it this is a preview",
        options: &[],
    },
    P_MIN_SIZE,
    P_EXT,
];


// ---------------------------------------------------------------------------
// glyph — Documents
// ---------------------------------------------------------------------------

const GLYPH_PDF: &[Param] = &[P_INPUT, P_OUT, P_TITLE];

const GLYPH_MARKDOWN: &[Param] = &[P_INPUT, P_OUT, P_TITLE];

// ---------------------------------------------------------------------------
// halo — Disc Studio
// ---------------------------------------------------------------------------

const HALO_CREATE: &[Param] = &[P_SOURCE, P_OUT, P_VOLUME_ID];

const HALO_EXTRACT: &[Param] = &[P_IMAGE, P_OUT, P_FILTER];

const HALO_CONVERT: &[Param] = &[P_SOURCE, P_OUT];

// One positional path, under the name the operation calls it.
const P_PATH_ONE: &[Param] = &[P_PATH];
const HALO_IMAGE_ONE: &[Param] = &[P_IMAGE];
const ARC_ONE: &[Param] = &[P_ARCHIVE];
const P_JOURNAL_REQ_ONE: &[Param] = &[P_JOURNAL_REQ];


// ---------------------------------------------------------------------------
// iris — Image Studio
// ---------------------------------------------------------------------------

const IRIS_CONVERT: &[Param] = &[P_PATH, P_OUT];
const IRIS_THUMB: &[Param] = &[P_PATH, P_OUT, P_WIDTH, P_HEIGHT];
const IRIS_TRANSFORM: &[Param] = &[P_PATH, P_OUT, P_ROTATE, P_FLIP_H, P_FLIP_V];
const IRIS_BATCH: &[Param] = &[P_PATH, P_OUT, P_FORMAT, P_SIZE];

// ---------------------------------------------------------------------------
// arc — Archives
// ---------------------------------------------------------------------------

const ARC_CREATE: &[Param] = &[P_PATHS, P_OUT, P_STORE];
const ARC_EXTRACT: &[Param] = &[P_ARCHIVE, P_OUT, P_FILTER];
// Required parameters first: the manifest test enforces it.
const ARC_SPLIT: &[Param] = &[P_PATH, P_OUT, P_SIZE];
const ARC_JOIN: &[Param] = &[P_INPUT, P_OUT_OPT];


// ---------------------------------------------------------------------------
// revive — Recovery
// ---------------------------------------------------------------------------

const REVIVE_SCAN: &[Param] = &[P_PATH, P_TYPES, P_MIN_SIZE, P_MAX_SIZE];
const REVIVE_CARVE: &[Param] = &[P_PATH, P_OUT, P_TYPES, P_MIN_SIZE, P_MAX_SIZE];
const REVIVE_REPORT: &[Param] = &[P_PATH, P_OUT_OPT];
const REVIVE_PACK: &[Param] = &[P_SOURCE, P_OUT];
const REVIVE_UNPACK: &[Param] = &[P_ARCHIVE, P_OUT];

// ---------------------------------------------------------------------------
// sift — the text half, from slate
// ---------------------------------------------------------------------------

const SIFT_VIEW: &[Param] = &[P_PATH, P_NUMBERS, P_START, P_COUNT];
const SIFT_EOL: &[Param] = &[P_PATH, P_EOL_TO, P_IN_PLACE];
const SIFT_CLEAN_OP: &[Param] = &[P_PATH, P_STRIP, P_TAB, P_COLLAPSE, P_IN_PLACE];
const SIFT_CONVERT: &[Param] = &[P_PATH, P_ENC_TO, P_OUT];


// ---------------------------------------------------------------------------
// forge — Binary Lab
// ---------------------------------------------------------------------------

const FORGE_HEXDUMP: &[Param] = &[P_PATH, P_OFFSET_OPT, P_LENGTH];
const FORGE_SEARCH: &[Param] = &[P_PATH, P_HEX];
const FORGE_PATCH: &[Param] = &[P_PATH, P_OFFSET, P_HEX, P_BACKUP];

// ---------------------------------------------------------------------------
// morph — Rename Engine
// ---------------------------------------------------------------------------

const MORPH_RENAME: &[Param] = &[
    P_PATHS, P_PATTERN_OPT, P_REPLACE, P_PREFIX, P_SUFFIX, P_CASE, P_EXT,
    P_EXT_LOWER, P_NUMBER, P_START, P_STEP, P_PAD, P_JOURNAL,
];


// ---------------------------------------------------------------------------
// aegis — Vault & Security
// ---------------------------------------------------------------------------

const AEGIS_CRYPTO: &[Param] = &[P_PATH, P_OUT_OPT, P_PASSPHRASE, P_KEYFILE];
const AEGIS_SCAN: &[Param] = &[P_PATH, P_SIGNATURES];

// ---------------------------------------------------------------------------
// pulse — System
// ---------------------------------------------------------------------------

const PULSE_TEMP: &[Param] = &[P_PATH_OPT, P_TOP];

// ---------------------------------------------------------------------------
// The manifest
// ---------------------------------------------------------------------------

/// Every module, in the order the rail shows them.
pub static MODULES: &[Module] = &[
    Module { name: "aegis",  title: "Vault & Security", job: "Encrypt, shred, scan and audit",              sources: "aegis + vault" },
    Module { name: "forge",  title: "Binary Lab",       job: "Inspect and patch executables",               sources: "forge" },
    Module { name: "halo",   title: "Disc Studio",      job: "Author, read and convert disc images",        sources: "halo + optic" },
    Module { name: "iris",   title: "Image Studio",     job: "Inspect, convert and transform images",       sources: "iris + prism" },
    Module { name: "arc",    title: "Archives",         job: "Pack, split, join and transfer",              sources: "arc + vortex" },
    Module { name: "sift",   title: "Text & Search",    job: "Find, extract and replace across files",      sources: "sift + slate" },
    Module { name: "glyph",  title: "Documents",        job: "PDF, Markdown and readability",               sources: "glyph" },
    Module { name: "scout",  title: "Filesystem",       job: "Catalogue, deduplicate and compare",          sources: "scout + strata" },
    Module { name: "morph",  title: "Rename Engine",    job: "Batch rename with preview and undo",          sources: "morph + strata" },
    Module { name: "pulse",  title: "System",           job: "Hardware, processes and maintenance",         sources: "pulse + beacon" },
    Module { name: "pager",  title: "Window Manager",   job: "Snap, pin and arrange windows",               sources: "pager + dock" },
    Module { name: "revive", title: "Recovery",         job: "Carve, recover and verify snapshots",         sources: "revive + chronos" },
    Module { name: "flux",   title: "Audio",            job: "Inspect, tag and shape audio",                sources: "flux" },
];

/// Every operation, module by module.
pub static OPS: &[Op] = &[
    // -- aegis -------------------------------------------------------------
    Op { id: "aegis.encrypt", module: "aegis", name: "encrypt", summary: "Encrypt a file with a passphrase or keyfile (AES-256-GCM)", danger: Danger::Safe, status: Status::Implemented, params: AEGIS_CRYPTO },
    Op { id: "aegis.decrypt", module: "aegis", name: "decrypt", summary: "Decrypt a container made by encrypt", danger: Danger::Safe, status: Status::Implemented, params: AEGIS_CRYPTO },
    Op { id: "aegis.shred", module: "aegis", name: "shred", summary: "Overwrite and remove a file (DoD 5220.22-M, 3 passes)", danger: Danger::Irreversible, status: Status::Implemented, params: P_PATH_ONE },
    Op { id: "aegis.scan", module: "aegis", name: "scan", summary: "Scan for signature and heuristic threats", danger: Danger::Safe, status: Status::Implemented, params: AEGIS_SCAN },
    Op { id: "aegis.persistence",module: "aegis", name: "persistence",summary: "Audit startup and persistence vectors",                    danger: Danger::Safe,         status: Status::Planned, params: &[] },
    Op { id: "aegis.harden", module: "aegis", name: "harden", summary: "Audit an executable for DEP, ASLR and signing", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },

    // -- forge -------------------------------------------------------------
    Op { id: "forge.inspect", module: "forge", name: "inspect", summary: "Read DOS, COFF and optional PE headers", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },
    Op { id: "forge.sections", module: "forge", name: "sections", summary: "List section headers with Shannon entropy", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },
    Op { id: "forge.imports", module: "forge", name: "imports", summary: "List imported dynamic libraries", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },
    Op { id: "forge.hexdump", module: "forge", name: "hexdump", summary: "Show bytes as hex and ASCII", danger: Danger::Safe, status: Status::Implemented, params: FORGE_HEXDUMP },
    Op { id: "forge.search", module: "forge", name: "search", summary: "Find a hex pattern in a binary", danger: Danger::Safe, status: Status::Implemented, params: FORGE_SEARCH },
    Op { id: "forge.patch", module: "forge", name: "patch", summary: "Overwrite bytes at an offset", danger: Danger::Irreversible, status: Status::Implemented, params: FORGE_PATCH },

    // -- halo --------------------------------------------------------------
    Op { id: "halo.create", module: "halo", name: "create", summary: "Build an ISO 9660 image from a directory", danger: Danger::Safe, status: Status::Implemented, params: HALO_CREATE },
    Op { id: "halo.list", module: "halo", name: "list", summary: "List the contents of a disc image", danger: Danger::Safe, status: Status::Implemented, params: HALO_IMAGE_ONE },
    Op { id: "halo.extract", module: "halo", name: "extract", summary: "Extract every file from a disc image", danger: Danger::Safe, status: Status::Implemented, params: HALO_EXTRACT },
    Op { id: "halo.verify", module: "halo", name: "verify", summary: "Check volume descriptors and checksums", danger: Danger::Safe, status: Status::Implemented, params: HALO_IMAGE_ONE },
    Op { id: "halo.convert", module: "halo", name: "convert", summary: "Convert raw 2352-byte sectors to 2048-byte ISO", danger: Danger::Safe, status: Status::Implemented, params: HALO_CONVERT },
    Op { id: "halo.cue",     module: "halo", name: "cue",     summary: "Write a CUE sheet for a disc image",            danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "halo.cover",   module: "halo", name: "cover",   summary: "Generate disc label and case artwork",          danger: Danger::Safe, status: Status::Planned, params: &[] },

    // -- iris --------------------------------------------------------------
    Op { id: "iris.info", module: "iris", name: "info", summary: "Dimensions, format, bit depth, aspect ratio", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },
    Op { id: "iris.convert", module: "iris", name: "convert", summary: "Convert between image formats", danger: Danger::Safe, status: Status::Implemented, params: IRIS_CONVERT },
    Op { id: "iris.thumb", module: "iris", name: "thumb", summary: "Downscale, keeping the aspect ratio", danger: Danger::Safe, status: Status::Implemented, params: IRIS_THUMB },
    Op { id: "iris.transform", module: "iris", name: "transform", summary: "Rotate and flip", danger: Danger::Safe, status: Status::Implemented, params: IRIS_TRANSFORM },
    Op { id: "iris.stats", module: "iris", name: "stats", summary: "Colour and luminance statistics", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },
    Op { id: "iris.batch", module: "iris", name: "batch", summary: "Apply one operation to a folder of images", danger: Danger::Safe, status: Status::Implemented, params: IRIS_BATCH },
    Op { id: "iris.ico",       module: "iris", name: "ico",       summary: "Build a multi-resolution Windows icon",       danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "iris.sheet",     module: "iris", name: "sheet",     summary: "Render a contact sheet",                      danger: Danger::Safe, status: Status::Planned, params: &[] },

    // -- arc ---------------------------------------------------------------
    Op { id: "arc.create", module: "arc", name: "create", summary: "Pack files into an archive", danger: Danger::Safe, status: Status::Implemented, params: ARC_CREATE },
    Op { id: "arc.extract", module: "arc", name: "extract", summary: "Unpack an archive", danger: Danger::Safe, status: Status::Implemented, params: ARC_EXTRACT },
    Op { id: "arc.list", module: "arc", name: "list", summary: "List archive contents", danger: Danger::Safe, status: Status::Implemented, params: ARC_ONE },
    Op { id: "arc.test", module: "arc", name: "test", summary: "Verify archive integrity", danger: Danger::Safe, status: Status::Implemented, params: ARC_ONE },
    Op { id: "arc.split", module: "arc", name: "split", summary: "Split a large file into volumes", danger: Danger::Safe, status: Status::Implemented, params: ARC_SPLIT },
    Op { id: "arc.join", module: "arc", name: "join", summary: "Reassemble split volumes", danger: Danger::Safe, status: Status::Implemented, params: ARC_JOIN },
    Op { id: "arc.chunk",   module: "arc", name: "chunk",   summary: "Plan a multi-segment download",             danger: Danger::Safe,         status: Status::Planned, params: &[] },
    Op { id: "arc.torrent", module: "arc", name: "torrent", summary: "Read metadata from a .torrent file",        danger: Danger::Safe,         status: Status::Planned, params: &[] },
    Op { id: "arc.probe",   module: "arc", name: "probe",   summary: "Probe a TCP endpoint and time it",          danger: Danger::Safe,         status: Status::Planned, params: &[] },
    Op { id: "arc.upnp",    module: "arc", name: "upnp",    summary: "Build UPnP port-mapping payloads",          danger: Danger::Safe,         status: Status::Planned, params: &[] },

    // -- sift --------------------------------------------------------------
    Op { id: "sift.strings", module: "sift", name: "strings", summary: "Extract printable ASCII and UTF-16 strings",     danger: Danger::Safe,         status: Status::Implemented, params: SIFT_STRINGS },
    Op { id: "sift.search",  module: "sift", name: "search",  summary: "Search files by literal or regular expression",   danger: Danger::Safe,         status: Status::Implemented, params: SIFT_SEARCH },
    Op { id: "sift.replace", module: "sift", name: "replace", summary: "Replace matches across files, with preview",      danger: Danger::Destructive,  status: Status::Implemented, params: SIFT_REPLACE },
    Op { id: "sift.xor",     module: "sift", name: "xor",     summary: "Recover XOR-obfuscated text from a binary",       danger: Danger::Safe,         status: Status::Implemented, params: SIFT_XOR },
    Op { id: "sift.view", module: "sift", name: "view", summary: "Read a text file with line numbers", danger: Danger::Safe, status: Status::Implemented, params: SIFT_VIEW },
    Op { id: "sift.eol", module: "sift", name: "eol", summary: "Normalise line endings", danger: Danger::Destructive, status: Status::Implemented, params: SIFT_EOL },
    Op { id: "sift.clean", module: "sift", name: "clean", summary: "Strip trailing whitespace and blank-line runs", danger: Danger::Destructive, status: Status::Implemented, params: SIFT_CLEAN_OP },
    Op { id: "sift.stats", module: "sift", name: "stats", summary: "Lines, words, characters, encoding", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },
    Op { id: "sift.convert", module: "sift", name: "convert", summary: "Convert text between encodings", danger: Danger::Safe, status: Status::Implemented, params: SIFT_CONVERT },

    // -- glyph -------------------------------------------------------------
    Op { id: "glyph.pdf", module: "glyph", name: "pdf", summary: "Generate a PDF from text", danger: Danger::Safe, status: Status::Implemented, params: GLYPH_PDF },
    Op { id: "glyph.extract", module: "glyph", name: "extract", summary: "Pull text out of a PDF", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },
    Op { id: "glyph.markdown", module: "glyph", name: "markdown", summary: "Render Markdown to styled HTML", danger: Danger::Safe, status: Status::Implemented, params: GLYPH_MARKDOWN },
    Op { id: "glyph.encoding", module: "glyph", name: "encoding", summary: "Detect encoding and line endings", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },
    Op { id: "glyph.readability", module: "glyph", name: "readability", summary: "Word count, reading time, readability", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },
    Op { id: "glyph.metrics", module: "glyph", name: "metrics", summary: "Code lines, comments, blanks, density", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },
    Op { id: "glyph.search",      module: "glyph", name: "search",      summary: "Search a document for text",                danger: Danger::Safe, status: Status::Planned, params: &[] },

    // -- scout -------------------------------------------------------------
    Op { id: "scout.catalog",    module: "scout", name: "catalog",    summary: "Catalogue a tree to table, JSON, CSV, Markdown or HTML", danger: Danger::Safe,         status: Status::Implemented, params: SCOUT_CATALOG },
    Op { id: "scout.duplicates", module: "scout", name: "duplicates", summary: "Find byte-identical files by SHA-256",                   danger: Danger::Safe,         status: Status::Implemented, params: SCOUT_DUPLICATES },
    Op { id: "scout.clean",      module: "scout", name: "clean",      summary: "Remove duplicate copies, preview first",                 danger: Danger::Irreversible, status: Status::Implemented, params: SCOUT_CLEAN },
    Op { id: "scout.treemap",    module: "scout", name: "treemap",    summary: "Render space usage as a treemap",                        danger: Danger::Safe,         status: Status::Planned,     params: &[] },
    Op { id: "scout.inventory",  module: "scout", name: "inventory",  summary: "Depth, capacity and largest files",                      danger: Danger::Safe,         status: Status::Planned,     params: &[] },
    Op { id: "scout.diff",       module: "scout", name: "diff",       summary: "Compare two files byte by byte",                         danger: Danger::Safe,         status: Status::Planned,     params: &[] },

    // -- morph -------------------------------------------------------------
    Op { id: "morph.rename", module: "morph", name: "rename", summary: "Rename by pattern with collision safety", danger: Danger::Destructive, status: Status::Implemented, params: MORPH_RENAME },
    Op { id: "morph.undo", module: "morph", name: "undo", summary: "Roll back the last rename transaction", danger: Danger::Destructive, status: Status::Implemented, params: P_JOURNAL_REQ_ONE },

    // -- pulse -------------------------------------------------------------
    Op { id: "pulse.hardware", module: "pulse", name: "hardware", summary: "CPU, memory and architecture",         danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "pulse.procs",    module: "pulse", name: "procs",    summary: "Running processes, threads, priority", danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "pulse.drives",   module: "pulse", name: "drives",   summary: "Drives, filesystems, free space",      danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "pulse.hash", module: "pulse", name: "hash", summary: "CRC32 and SHA-256 of a file", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },
    Op { id: "pulse.temp", module: "pulse", name: "temp", summary: "Audit reclaimable temporary storage", danger: Danger::Safe, status: Status::Implemented, params: PULSE_TEMP },
    Op { id: "pulse.services", module: "pulse", name: "services", summary: "Audit background services",            danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "pulse.priority", module: "pulse", name: "priority", summary: "Change a process priority class",      danger: Danger::Destructive, status: Status::Planned, params: &[] },

    // -- pager -------------------------------------------------------------
    Op { id: "pager.list",   module: "pager", name: "list",   summary: "List visible top-level windows",      danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "pager.switch", module: "pager", name: "switch", summary: "Switch virtual desktop",               danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "pager.rollup", module: "pager", name: "rollup", summary: "Shade a window to its title bar",      danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "pager.pin",    module: "pager", name: "pin",    summary: "Keep a window above others",           danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "pager.alpha",  module: "pager", name: "alpha",  summary: "Set window transparency",              danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "pager.snap",   module: "pager", name: "snap",   summary: "Snap a window to a grid region",       danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "pager.move",   module: "pager", name: "move",   summary: "Move a window to a position or screen",danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "pager.launch", module: "pager", name: "launch", summary: "Start an application detached",        danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "pager.calc",   module: "pager", name: "calc",   summary: "Compute grid snap bounds",             danger: Danger::Safe, status: Status::Planned, params: &[] },

    // -- revive ------------------------------------------------------------
    Op { id: "revive.scan", module: "revive", name: "scan", summary: "Scan an image for recoverable files", danger: Danger::Safe, status: Status::Implemented, params: REVIVE_SCAN },
    Op { id: "revive.carve", module: "revive", name: "carve", summary: "Carve files out by signature", danger: Danger::Safe, status: Status::Implemented, params: REVIVE_CARVE },
    Op { id: "revive.report", module: "revive", name: "report", summary: "Summarise a carving run", danger: Danger::Safe, status: Status::Implemented, params: REVIVE_REPORT },
    Op { id: "revive.pack", module: "revive", name: "pack", summary: "Pack a tree into a snapshot", danger: Danger::Safe, status: Status::Implemented, params: REVIVE_PACK },
    Op { id: "revive.unpack", module: "revive", name: "unpack", summary: "Restore a snapshot", danger: Danger::Safe, status: Status::Implemented, params: REVIVE_UNPACK },
    Op { id: "revive.verify", module: "revive", name: "verify", summary: "Check a snapshot's checksums", danger: Danger::Safe, status: Status::Implemented, params: ARC_ONE },
    Op { id: "revive.geometry", module: "revive", name: "geometry", summary: "Read partition headers and entropy", danger: Danger::Safe, status: Status::Implemented, params: P_PATH_ONE },

    // -- flux --------------------------------------------------------------
    Op { id: "flux.inspect", module: "flux", name: "inspect", summary: "Format, sample rate, channels, ID3 tags", danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "flux.synth",   module: "flux", name: "synth",   summary: "Synthesise a PCM WAV tone",               danger: Danger::Safe, status: Status::Planned, params: &[] },
    Op { id: "flux.tag",     module: "flux", name: "tag",     summary: "Write ID3 metadata",                      danger: Danger::Destructive, status: Status::Planned, params: &[] },
    Op { id: "flux.gain",    module: "flux", name: "gain",    summary: "Scale PCM volume",                        danger: Danger::Destructive, status: Status::Planned, params: &[] },
];

// ---------------------------------------------------------------------------
// Lookup
// ---------------------------------------------------------------------------

/// Find a module by its short name.
pub fn module(name: &str) -> Option<&'static Module> {
    MODULES.iter().find(|m| m.name == name)
}

/// Find an operation by `module.name`, or by its bare name within a module.
pub fn op(module_name: &str, op_name: &str) -> Option<&'static Op> {
    OPS.iter()
        .find(|o| o.module == module_name && o.name == op_name)
}

/// Every operation belonging to one module.
pub fn ops_in(module_name: &str) -> Vec<&'static Op> {
    OPS.iter().filter(|o| o.module == module_name).collect()
}

/// How many operations are declared, and how many actually run.
pub fn counts() -> (usize, usize) {
    let implemented = OPS
        .iter()
        .filter(|o| o.status == Status::Implemented)
        .count();
    (OPS.len(), implemented)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn manifest_declares_eighty_eight_operations() {
        let (total, implemented) = counts();
        assert_eq!(total, 88, "the plan promises 88 operations");
        assert_eq!(implemented, 57, "sift 9, scout 3, glyph 6, halo 5, iris 6, arc 6, revive 7, forge 6, morph 2, aegis 5, pulse 2");
    }

    #[test]
    fn every_operation_id_is_unique() {
        let mut seen = HashSet::new();
        for o in OPS {
            assert!(seen.insert(o.id), "duplicate operation id: {}", o.id);
        }
    }

    #[test]
    fn every_operation_belongs_to_a_declared_module() {
        for o in OPS {
            assert!(
                module(o.module).is_some(),
                "operation {} names module '{}' which is not declared",
                o.id,
                o.module
            );
        }
    }

    #[test]
    fn operation_id_matches_module_and_name() {
        for o in OPS {
            assert_eq!(
                o.id,
                format!("{}.{}", o.module, o.name),
                "id must be module.name"
            );
        }
    }

    #[test]
    fn every_module_has_operations() {
        for m in MODULES {
            assert!(!ops_in(m.name).is_empty(), "module {} is empty", m.name);
        }
    }

    #[test]
    fn implemented_operations_declare_their_parameters() {
        for o in OPS.iter().filter(|o| o.status == Status::Implemented) {
            assert!(!o.params.is_empty(), "{} is implemented but declares no parameters", o.id);
            let positional = o.params.iter().filter(|p| p.positional).count();
            assert!(positional >= 1, "{} needs at least one positional parameter", o.id);
        }
    }

    #[test]
    fn required_parameters_come_before_optional_ones() {
        for o in OPS {
            let mut seen_optional = false;
            for p in o.params {
                if !p.required {
                    seen_optional = true;
                } else {
                    assert!(
                        !seen_optional,
                        "{}: required parameter '{}' follows an optional one",
                        o.id, p.name
                    );
                }
            }
        }
    }

    #[test]
    fn lookup_finds_a_known_operation() {
        let found = op("sift", "search").expect("sift.search is declared");
        assert_eq!(found.danger, Danger::Safe);
        assert_eq!(found.status, Status::Implemented);
        assert!(op("sift", "nonsense").is_none());
    }

    #[test]
    fn destructive_operations_are_marked() {
        // Anything that writes in place must not be filed as safe.
        for id in [
            "sift.replace",
            "scout.clean",
            "aegis.shred",
            "forge.patch",
            "morph.rename",
        ] {
            let found = OPS.iter().find(|o| o.id == id).expect("declared");
            assert_ne!(found.danger, Danger::Safe, "{} must not be marked safe", id);
        }
    }
}
