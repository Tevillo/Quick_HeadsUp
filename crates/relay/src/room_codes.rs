//! Hardcoded pool of ASOIAF room codes — single-word alphabetic entries ≤8
//! chars, filtered from `lists/ASOIAF_list.txt` across all categories. Stored
//! uppercase so they match the case-insensitive lookup path.

pub const POOL: &[&str] = &[
    "ANGUY", "ASSHAI", "ASTAPOR", "BRAVOS", "BRONN", "CANNIBAL", "CARAXES", "CRASTER", "DAWN",
    "DORNE", "DROGON", "DUCK", "EGG", "ESSOS", "GENDRY", "GHOST", "GILLY", "GRENN", "HARDHOME",
    "HODOR", "HOTPIE", "IB", "ICE", "KARHOLD", "LADY", "LEATHERS", "LONGCLAW", "LORATH", "LYS",
    "MELEYS", "MEREEN", "MOQORRO", "MYR", "NAARTH", "NYMERIA", "OLDTOWN", "OPPO", "ORELL", "PATE",
    "PENNY", "PENTOS", "POLLIVER", "PYKE", "PYP", "QARTH", "QOHOR", "QUAITHE", "QYBURN", "REEK",
    "RHAEGAL", "RIVERRUN", "SALTPANS", "SEASMOKE", "SKAGOS", "STARFALL", "SUMMER", "SUNFYRE",
    "SUNSPEAR", "SYRAX", "THOROS", "TIMETT", "TYROSH", "TYSHA", "VAL", "VALYRIA", "VARYS",
    "VERMAX", "VHAGAR", "VISERYON", "VOLANTIS", "WESTEROS", "YEEN", "YGRITTE", "YUNKAI",
];

pub const MAX_POOL_ATTEMPTS: usize = 8;
