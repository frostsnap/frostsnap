//\! BIP39 English word list

/// Number of BIP39 words used in Frostsnap backup
pub const FROSTSNAP_BACKUP_WORDS: usize = 25;

/// Check if a word is in the BIP39 word list using binary search
pub fn is_valid_bip39_word(word: &str) -> bool {
    BIP39_WORDS.binary_search(&word).is_ok()
}

/// Get all words that start with the given prefix
pub fn words_with_prefix(prefix: &str) -> &'static [&'static str] {
    if prefix.is_empty() {
        return &BIP39_WORDS;
    }

    let start = BIP39_WORDS.partition_point(|w| w < &prefix);

    // Find the end of the matching words
    let mut end = start;
    while end < BIP39_WORDS.len() && BIP39_WORDS[end].starts_with(prefix) {
        end += 1;
    }

    &BIP39_WORDS[start..end]
}

/// Returns which next letters are possible after `prefix` in the BIP39 list.
pub fn get_valid_next_letters(prefix: &str) -> ValidLetters {
    if prefix.is_empty() {
        // For empty prefix, return the default which has all letters except X
        return ValidLetters::default();
    }
    // 1) Lower bound: first index where word >= prefix
    let start = BIP39_WORDS.partition_point(|w| &w[..prefix.len().min(w.len())] < prefix);
    let mut valid = ValidLetters::all_false();

    // 2) Walk forward, strip off the prefix, and collect the very next char
    for &word in &BIP39_WORDS[start..] {
        if let Some(rest) = word.strip_prefix(prefix) {
            if let Some(ch) = rest.chars().next() {
                valid.set(ch);
            }
        } else {
            // as soon as strip_prefix fails, we’re past the matching block
            break;
        }
    }

    valid
}

/// Represents which letters (A-Z) are valid next characters
#[derive(Debug, Clone, Copy)]
pub struct ValidLetters {
    letters: [bool; 26],
    count: u8, // Cache the count of enabled letters
}

const DEFAULT_VALID_LETTERS: [bool; 26] = [
    true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, false, /* no letter starts with x */
    true, true,
];

impl Default for ValidLetters {
    fn default() -> Self {
        let count = DEFAULT_VALID_LETTERS.iter().filter(|&&b| b).count() as u8;
        Self {
            letters: DEFAULT_VALID_LETTERS,
            count,
        }
    }
}

impl ValidLetters {
    /// Create a new ValidLetters with all letters invalid
    pub fn all_false() -> Self {
        Self {
            letters: [false; 26],
            count: 0,
        }
    }

    /// Create a new ValidLetters with all letters valid
    pub fn all_valid() -> Self {
        Self {
            letters: [true; 26],
            count: 26,
        }
    }

    /// Set a letter as valid (letter should be uppercase A-Z)
    pub fn set(&mut self, letter: char) {
        if let Some(idx) = Self::letter_to_index(letter) {
            if !self.letters[idx] {
                self.letters[idx] = true;
                self.count += 1;
            }
        }
    }

    /// Check if a letter is valid
    pub fn is_valid(&self, letter: char) -> bool {
        Self::letter_to_index(letter)
            .map(|idx| self.letters[idx])
            .unwrap_or(false)
    }

    /// Get the nth enabled letter (returns None if index is out of bounds)
    pub fn nth_enabled(&self, n: usize) -> Option<char> {
        let mut count = 0;
        for (idx, &is_valid) in self.letters.iter().enumerate() {
            if is_valid {
                if count == n {
                    return Some((b'A' + idx as u8) as char);
                }
                count += 1;
            }
        }
        None
    }

    /// Count the number of enabled letters (cached)
    pub fn count_enabled(&self) -> usize {
        self.count as usize
    }

    /// Returns an iterator over all valid letters
    pub fn iter_valid(&self) -> impl Iterator<Item = char> + '_ {
        self.letters
            .iter()
            .enumerate()
            .filter_map(|(idx, &is_valid)| {
                if is_valid {
                    Some((b'A' + idx as u8) as char)
                } else {
                    None
                }
            })
    }

    fn letter_to_index(letter: char) -> Option<usize> {
        match letter {
            'A'..='Z' => Some((letter as u8 - b'A') as usize),
            _ => None,
        }
    }
}

/// The complete BIP39 English word list (2048 words)
pub static BIP39_WORDS: [&str; 2048] = [
    "ABANDON", "ABILITY", "ABLE", "ABOUT", "ABOVE", "ABSENT", "ABSORB", "ABSTRACT", "ABSURD",
    "ABUSE", "ACCESS", "ACCIDENT", "ACCOUNT", "ACCUSE", "ACHIEVE", "ACID", "ACOUSTIC", "ACQUIRE",
    "ACROSS", "ACT", "ACTION", "ACTOR", "ACTRESS", "ACTUAL", "ADAPT", "ADD", "ADDICT", "ADDRESS",
    "ADJUST", "ADMIT", "ADULT", "ADVANCE", "ADVICE", "AEROBIC", "AFFAIR", "AFFORD", "AFRAID",
    "AGAIN", "AGE", "AGENT", "AGREE", "AHEAD", "AIM", "AIR", "AIRPORT", "AISLE", "ALARM", "ALBUM",
    "ALCOHOL", "ALERT", "ALIEN", "ALL", "ALLEY", "ALLOW", "ALMOST", "ALONE", "ALPHA", "ALREADY",
    "ALSO", "ALTER", "ALWAYS", "AMATEUR", "AMAZING", "AMONG", "AMOUNT", "AMUSED", "ANALYST",
    "ANCHOR", "ANCIENT", "ANGER", "ANGLE", "ANGRY", "ANIMAL", "ANKLE", "ANNOUNCE", "ANNUAL",
    "ANOTHER", "ANSWER", "ANTENNA", "ANTIQUE", "ANXIETY", "ANY", "APART", "APOLOGY", "APPEAR",
    "APPLE", "APPROVE", "APRIL", "ARCH", "ARCTIC", "AREA", "ARENA", "ARGUE", "ARM", "ARMED",
    "ARMOR", "ARMY", "AROUND", "ARRANGE", "ARREST", "ARRIVE", "ARROW", "ART", "ARTEFACT", "ARTIST",
    "ARTWORK", "ASK", "ASPECT", "ASSAULT", "ASSET", "ASSIST", "ASSUME", "ASTHMA", "ATHLETE",
    "ATOM", "ATTACK", "ATTEND", "ATTITUDE", "ATTRACT", "AUCTION", "AUDIT", "AUGUST", "AUNT",
    "AUTHOR", "AUTO", "AUTUMN", "AVERAGE", "AVOCADO", "AVOID", "AWAKE", "AWARE", "AWAY", "AWESOME",
    "AWFUL", "AWKWARD", "AXIS", "BABY", "BACHELOR", "BACON", "BADGE", "BAG", "BALANCE", "BALCONY",
    "BALL", "BAMBOO", "BANANA", "BANNER", "BAR", "BARELY", "BARGAIN", "BARREL", "BASE", "BASIC",
    "BASKET", "BATTLE", "BEACH", "BEAN", "BEAUTY", "BECAUSE", "BECOME", "BEEF", "BEFORE", "BEGIN",
    "BEHAVE", "BEHIND", "BELIEVE", "BELOW", "BELT", "BENCH", "BENEFIT", "BEST", "BETRAY", "BETTER",
    "BETWEEN", "BEYOND", "BICYCLE", "BID", "BIKE", "BIND", "BIOLOGY", "BIRD", "BIRTH", "BITTER",
    "BLACK", "BLADE", "BLAME", "BLANKET", "BLAST", "BLEAK", "BLESS", "BLIND", "BLOOD", "BLOSSOM",
    "BLOUSE", "BLUE", "BLUR", "BLUSH", "BOARD", "BOAT", "BODY", "BOIL", "BOMB", "BONE", "BONUS",
    "BOOK", "BOOST", "BORDER", "BORING", "BORROW", "BOSS", "BOTTOM", "BOUNCE", "BOX", "BOY",
    "BRACKET", "BRAIN", "BRAND", "BRASS", "BRAVE", "BREAD", "BREEZE", "BRICK", "BRIDGE", "BRIEF",
    "BRIGHT", "BRING", "BRISK", "BROCCOLI", "BROKEN", "BRONZE", "BROOM", "BROTHER", "BROWN",
    "BRUSH", "BUBBLE", "BUDDY", "BUDGET", "BUFFALO", "BUILD", "BULB", "BULK", "BULLET", "BUNDLE",
    "BUNKER", "BURDEN", "BURGER", "BURST", "BUS", "BUSINESS", "BUSY", "BUTTER", "BUYER", "BUZZ",
    "CABBAGE", "CABIN", "CABLE", "CACTUS", "CAGE", "CAKE", "CALL", "CALM", "CAMERA", "CAMP", "CAN",
    "CANAL", "CANCEL", "CANDY", "CANNON", "CANOE", "CANVAS", "CANYON", "CAPABLE", "CAPITAL",
    "CAPTAIN", "CAR", "CARBON", "CARD", "CARGO", "CARPET", "CARRY", "CART", "CASE", "CASH",
    "CASINO", "CASTLE", "CASUAL", "CAT", "CATALOG", "CATCH", "CATEGORY", "CATTLE", "CAUGHT",
    "CAUSE", "CAUTION", "CAVE", "CEILING", "CELERY", "CEMENT", "CENSUS", "CENTURY", "CEREAL",
    "CERTAIN", "CHAIR", "CHALK", "CHAMPION", "CHANGE", "CHAOS", "CHAPTER", "CHARGE", "CHASE",
    "CHAT", "CHEAP", "CHECK", "CHEESE", "CHEF", "CHERRY", "CHEST", "CHICKEN", "CHIEF", "CHILD",
    "CHIMNEY", "CHOICE", "CHOOSE", "CHRONIC", "CHUCKLE", "CHUNK", "CHURN", "CIGAR", "CINNAMON",
    "CIRCLE", "CITIZEN", "CITY", "CIVIL", "CLAIM", "CLAP", "CLARIFY", "CLAW", "CLAY", "CLEAN",
    "CLERK", "CLEVER", "CLICK", "CLIENT", "CLIFF", "CLIMB", "CLINIC", "CLIP", "CLOCK", "CLOG",
    "CLOSE", "CLOTH", "CLOUD", "CLOWN", "CLUB", "CLUMP", "CLUSTER", "CLUTCH", "COACH", "COAST",
    "COCONUT", "CODE", "COFFEE", "COIL", "COIN", "COLLECT", "COLOR", "COLUMN", "COMBINE", "COME",
    "COMFORT", "COMIC", "COMMON", "COMPANY", "CONCERT", "CONDUCT", "CONFIRM", "CONGRESS",
    "CONNECT", "CONSIDER", "CONTROL", "CONVINCE", "COOK", "COOL", "COPPER", "COPY", "CORAL",
    "CORE", "CORN", "CORRECT", "COST", "COTTON", "COUCH", "COUNTRY", "COUPLE", "COURSE", "COUSIN",
    "COVER", "COYOTE", "CRACK", "CRADLE", "CRAFT", "CRAM", "CRANE", "CRASH", "CRATER", "CRAWL",
    "CRAZY", "CREAM", "CREDIT", "CREEK", "CREW", "CRICKET", "CRIME", "CRISP", "CRITIC", "CROP",
    "CROSS", "CROUCH", "CROWD", "CRUCIAL", "CRUEL", "CRUISE", "CRUMBLE", "CRUNCH", "CRUSH", "CRY",
    "CRYSTAL", "CUBE", "CULTURE", "CUP", "CUPBOARD", "CURIOUS", "CURRENT", "CURTAIN", "CURVE",
    "CUSHION", "CUSTOM", "CUTE", "CYCLE", "DAD", "DAMAGE", "DAMP", "DANCE", "DANGER", "DARING",
    "DASH", "DAUGHTER", "DAWN", "DAY", "DEAL", "DEBATE", "DEBRIS", "DECADE", "DECEMBER", "DECIDE",
    "DECLINE", "DECORATE", "DECREASE", "DEER", "DEFENSE", "DEFINE", "DEFY", "DEGREE", "DELAY",
    "DELIVER", "DEMAND", "DEMISE", "DENIAL", "DENTIST", "DENY", "DEPART", "DEPEND", "DEPOSIT",
    "DEPTH", "DEPUTY", "DERIVE", "DESCRIBE", "DESERT", "DESIGN", "DESK", "DESPAIR", "DESTROY",
    "DETAIL", "DETECT", "DEVELOP", "DEVICE", "DEVOTE", "DIAGRAM", "DIAL", "DIAMOND", "DIARY",
    "DICE", "DIESEL", "DIET", "DIFFER", "DIGITAL", "DIGNITY", "DILEMMA", "DINNER", "DINOSAUR",
    "DIRECT", "DIRT", "DISAGREE", "DISCOVER", "DISEASE", "DISH", "DISMISS", "DISORDER", "DISPLAY",
    "DISTANCE", "DIVERT", "DIVIDE", "DIVORCE", "DIZZY", "DOCTOR", "DOCUMENT", "DOG", "DOLL",
    "DOLPHIN", "DOMAIN", "DONATE", "DONKEY", "DONOR", "DOOR", "DOSE", "DOUBLE", "DOVE", "DRAFT",
    "DRAGON", "DRAMA", "DRASTIC", "DRAW", "DREAM", "DRESS", "DRIFT", "DRILL", "DRINK", "DRIP",
    "DRIVE", "DROP", "DRUM", "DRY", "DUCK", "DUMB", "DUNE", "DURING", "DUST", "DUTCH", "DUTY",
    "DWARF", "DYNAMIC", "EAGER", "EAGLE", "EARLY", "EARN", "EARTH", "EASILY", "EAST", "EASY",
    "ECHO", "ECOLOGY", "ECONOMY", "EDGE", "EDIT", "EDUCATE", "EFFORT", "EGG", "EIGHT", "EITHER",
    "ELBOW", "ELDER", "ELECTRIC", "ELEGANT", "ELEMENT", "ELEPHANT", "ELEVATOR", "ELITE", "ELSE",
    "EMBARK", "EMBODY", "EMBRACE", "EMERGE", "EMOTION", "EMPLOY", "EMPOWER", "EMPTY", "ENABLE",
    "ENACT", "END", "ENDLESS", "ENDORSE", "ENEMY", "ENERGY", "ENFORCE", "ENGAGE", "ENGINE",
    "ENHANCE", "ENJOY", "ENLIST", "ENOUGH", "ENRICH", "ENROLL", "ENSURE", "ENTER", "ENTIRE",
    "ENTRY", "ENVELOPE", "EPISODE", "EQUAL", "EQUIP", "ERA", "ERASE", "ERODE", "EROSION", "ERROR",
    "ERUPT", "ESCAPE", "ESSAY", "ESSENCE", "ESTATE", "ETERNAL", "ETHICS", "EVIDENCE", "EVIL",
    "EVOKE", "EVOLVE", "EXACT", "EXAMPLE", "EXCESS", "EXCHANGE", "EXCITE", "EXCLUDE", "EXCUSE",
    "EXECUTE", "EXERCISE", "EXHAUST", "EXHIBIT", "EXILE", "EXIST", "EXIT", "EXOTIC", "EXPAND",
    "EXPECT", "EXPIRE", "EXPLAIN", "EXPOSE", "EXPRESS", "EXTEND", "EXTRA", "EYE", "EYEBROW",
    "FABRIC", "FACE", "FACULTY", "FADE", "FAINT", "FAITH", "FALL", "FALSE", "FAME", "FAMILY",
    "FAMOUS", "FAN", "FANCY", "FANTASY", "FARM", "FASHION", "FAT", "FATAL", "FATHER", "FATIGUE",
    "FAULT", "FAVORITE", "FEATURE", "FEBRUARY", "FEDERAL", "FEE", "FEED", "FEEL", "FEMALE",
    "FENCE", "FESTIVAL", "FETCH", "FEVER", "FEW", "FIBER", "FICTION", "FIELD", "FIGURE", "FILE",
    "FILM", "FILTER", "FINAL", "FIND", "FINE", "FINGER", "FINISH", "FIRE", "FIRM", "FIRST",
    "FISCAL", "FISH", "FIT", "FITNESS", "FIX", "FLAG", "FLAME", "FLASH", "FLAT", "FLAVOR", "FLEE",
    "FLIGHT", "FLIP", "FLOAT", "FLOCK", "FLOOR", "FLOWER", "FLUID", "FLUSH", "FLY", "FOAM",
    "FOCUS", "FOG", "FOIL", "FOLD", "FOLLOW", "FOOD", "FOOT", "FORCE", "FOREST", "FORGET", "FORK",
    "FORTUNE", "FORUM", "FORWARD", "FOSSIL", "FOSTER", "FOUND", "FOX", "FRAGILE", "FRAME",
    "FREQUENT", "FRESH", "FRIEND", "FRINGE", "FROG", "FRONT", "FROST", "FROWN", "FROZEN", "FRUIT",
    "FUEL", "FUN", "FUNNY", "FURNACE", "FURY", "FUTURE", "GADGET", "GAIN", "GALAXY", "GALLERY",
    "GAME", "GAP", "GARAGE", "GARBAGE", "GARDEN", "GARLIC", "GARMENT", "GAS", "GASP", "GATE",
    "GATHER", "GAUGE", "GAZE", "GENERAL", "GENIUS", "GENRE", "GENTLE", "GENUINE", "GESTURE",
    "GHOST", "GIANT", "GIFT", "GIGGLE", "GINGER", "GIRAFFE", "GIRL", "GIVE", "GLAD", "GLANCE",
    "GLARE", "GLASS", "GLIDE", "GLIMPSE", "GLOBE", "GLOOM", "GLORY", "GLOVE", "GLOW", "GLUE",
    "GOAT", "GODDESS", "GOLD", "GOOD", "GOOSE", "GORILLA", "GOSPEL", "GOSSIP", "GOVERN", "GOWN",
    "GRAB", "GRACE", "GRAIN", "GRANT", "GRAPE", "GRASS", "GRAVITY", "GREAT", "GREEN", "GRID",
    "GRIEF", "GRIT", "GROCERY", "GROUP", "GROW", "GRUNT", "GUARD", "GUESS", "GUIDE", "GUILT",
    "GUITAR", "GUN", "GYM", "HABIT", "HAIR", "HALF", "HAMMER", "HAMSTER", "HAND", "HAPPY",
    "HARBOR", "HARD", "HARSH", "HARVEST", "HAT", "HAVE", "HAWK", "HAZARD", "HEAD", "HEALTH",
    "HEART", "HEAVY", "HEDGEHOG", "HEIGHT", "HELLO", "HELMET", "HELP", "HEN", "HERO", "HIDDEN",
    "HIGH", "HILL", "HINT", "HIP", "HIRE", "HISTORY", "HOBBY", "HOCKEY", "HOLD", "HOLE", "HOLIDAY",
    "HOLLOW", "HOME", "HONEY", "HOOD", "HOPE", "HORN", "HORROR", "HORSE", "HOSPITAL", "HOST",
    "HOTEL", "HOUR", "HOVER", "HUB", "HUGE", "HUMAN", "HUMBLE", "HUMOR", "HUNDRED", "HUNGRY",
    "HUNT", "HURDLE", "HURRY", "HURT", "HUSBAND", "HYBRID", "ICE", "ICON", "IDEA", "IDENTIFY",
    "IDLE", "IGNORE", "ILL", "ILLEGAL", "ILLNESS", "IMAGE", "IMITATE", "IMMENSE", "IMMUNE",
    "IMPACT", "IMPOSE", "IMPROVE", "IMPULSE", "INCH", "INCLUDE", "INCOME", "INCREASE", "INDEX",
    "INDICATE", "INDOOR", "INDUSTRY", "INFANT", "INFLICT", "INFORM", "INHALE", "INHERIT",
    "INITIAL", "INJECT", "INJURY", "INMATE", "INNER", "INNOCENT", "INPUT", "INQUIRY", "INSANE",
    "INSECT", "INSIDE", "INSPIRE", "INSTALL", "INTACT", "INTEREST", "INTO", "INVEST", "INVITE",
    "INVOLVE", "IRON", "ISLAND", "ISOLATE", "ISSUE", "ITEM", "IVORY", "JACKET", "JAGUAR", "JAR",
    "JAZZ", "JEALOUS", "JEANS", "JELLY", "JEWEL", "JOB", "JOIN", "JOKE", "JOURNEY", "JOY", "JUDGE",
    "JUICE", "JUMP", "JUNGLE", "JUNIOR", "JUNK", "JUST", "KANGAROO", "KEEN", "KEEP", "KETCHUP",
    "KEY", "KICK", "KID", "KIDNEY", "KIND", "KINGDOM", "KISS", "KIT", "KITCHEN", "KITE", "KITTEN",
    "KIWI", "KNEE", "KNIFE", "KNOCK", "KNOW", "LAB", "LABEL", "LABOR", "LADDER", "LADY", "LAKE",
    "LAMP", "LANGUAGE", "LAPTOP", "LARGE", "LATER", "LATIN", "LAUGH", "LAUNDRY", "LAVA", "LAW",
    "LAWN", "LAWSUIT", "LAYER", "LAZY", "LEADER", "LEAF", "LEARN", "LEAVE", "LECTURE", "LEFT",
    "LEG", "LEGAL", "LEGEND", "LEISURE", "LEMON", "LEND", "LENGTH", "LENS", "LEOPARD", "LESSON",
    "LETTER", "LEVEL", "LIAR", "LIBERTY", "LIBRARY", "LICENSE", "LIFE", "LIFT", "LIGHT", "LIKE",
    "LIMB", "LIMIT", "LINK", "LION", "LIQUID", "LIST", "LITTLE", "LIVE", "LIZARD", "LOAD", "LOAN",
    "LOBSTER", "LOCAL", "LOCK", "LOGIC", "LONELY", "LONG", "LOOP", "LOTTERY", "LOUD", "LOUNGE",
    "LOVE", "LOYAL", "LUCKY", "LUGGAGE", "LUMBER", "LUNAR", "LUNCH", "LUXURY", "LYRICS", "MACHINE",
    "MAD", "MAGIC", "MAGNET", "MAID", "MAIL", "MAIN", "MAJOR", "MAKE", "MAMMAL", "MAN", "MANAGE",
    "MANDATE", "MANGO", "MANSION", "MANUAL", "MAPLE", "MARBLE", "MARCH", "MARGIN", "MARINE",
    "MARKET", "MARRIAGE", "MASK", "MASS", "MASTER", "MATCH", "MATERIAL", "MATH", "MATRIX",
    "MATTER", "MAXIMUM", "MAZE", "MEADOW", "MEAN", "MEASURE", "MEAT", "MECHANIC", "MEDAL", "MEDIA",
    "MELODY", "MELT", "MEMBER", "MEMORY", "MENTION", "MENU", "MERCY", "MERGE", "MERIT", "MERRY",
    "MESH", "MESSAGE", "METAL", "METHOD", "MIDDLE", "MIDNIGHT", "MILK", "MILLION", "MIMIC", "MIND",
    "MINIMUM", "MINOR", "MINUTE", "MIRACLE", "MIRROR", "MISERY", "MISS", "MISTAKE", "MIX", "MIXED",
    "MIXTURE", "MOBILE", "MODEL", "MODIFY", "MOM", "MOMENT", "MONITOR", "MONKEY", "MONSTER",
    "MONTH", "MOON", "MORAL", "MORE", "MORNING", "MOSQUITO", "MOTHER", "MOTION", "MOTOR",
    "MOUNTAIN", "MOUSE", "MOVE", "MOVIE", "MUCH", "MUFFIN", "MULE", "MULTIPLY", "MUSCLE", "MUSEUM",
    "MUSHROOM", "MUSIC", "MUST", "MUTUAL", "MYSELF", "MYSTERY", "MYTH", "NAIVE", "NAME", "NAPKIN",
    "NARROW", "NASTY", "NATION", "NATURE", "NEAR", "NECK", "NEED", "NEGATIVE", "NEGLECT",
    "NEITHER", "NEPHEW", "NERVE", "NEST", "NET", "NETWORK", "NEUTRAL", "NEVER", "NEWS", "NEXT",
    "NICE", "NIGHT", "NOBLE", "NOISE", "NOMINEE", "NOODLE", "NORMAL", "NORTH", "NOSE", "NOTABLE",
    "NOTE", "NOTHING", "NOTICE", "NOVEL", "NOW", "NUCLEAR", "NUMBER", "NURSE", "NUT", "OAK",
    "OBEY", "OBJECT", "OBLIGE", "OBSCURE", "OBSERVE", "OBTAIN", "OBVIOUS", "OCCUR", "OCEAN",
    "OCTOBER", "ODOR", "OFF", "OFFER", "OFFICE", "OFTEN", "OIL", "OKAY", "OLD", "OLIVE", "OLYMPIC",
    "OMIT", "ONCE", "ONE", "ONION", "ONLINE", "ONLY", "OPEN", "OPERA", "OPINION", "OPPOSE",
    "OPTION", "ORANGE", "ORBIT", "ORCHARD", "ORDER", "ORDINARY", "ORGAN", "ORIENT", "ORIGINAL",
    "ORPHAN", "OSTRICH", "OTHER", "OUTDOOR", "OUTER", "OUTPUT", "OUTSIDE", "OVAL", "OVEN", "OVER",
    "OWN", "OWNER", "OXYGEN", "OYSTER", "OZONE", "PACT", "PADDLE", "PAGE", "PAIR", "PALACE",
    "PALM", "PANDA", "PANEL", "PANIC", "PANTHER", "PAPER", "PARADE", "PARENT", "PARK", "PARROT",
    "PARTY", "PASS", "PATCH", "PATH", "PATIENT", "PATROL", "PATTERN", "PAUSE", "PAVE", "PAYMENT",
    "PEACE", "PEANUT", "PEAR", "PEASANT", "PELICAN", "PEN", "PENALTY", "PENCIL", "PEOPLE",
    "PEPPER", "PERFECT", "PERMIT", "PERSON", "PET", "PHONE", "PHOTO", "PHRASE", "PHYSICAL",
    "PIANO", "PICNIC", "PICTURE", "PIECE", "PIG", "PIGEON", "PILL", "PILOT", "PINK", "PIONEER",
    "PIPE", "PISTOL", "PITCH", "PIZZA", "PLACE", "PLANET", "PLASTIC", "PLATE", "PLAY", "PLEASE",
    "PLEDGE", "PLUCK", "PLUG", "PLUNGE", "POEM", "POET", "POINT", "POLAR", "POLE", "POLICE",
    "POND", "PONY", "POOL", "POPULAR", "PORTION", "POSITION", "POSSIBLE", "POST", "POTATO",
    "POTTERY", "POVERTY", "POWDER", "POWER", "PRACTICE", "PRAISE", "PREDICT", "PREFER", "PREPARE",
    "PRESENT", "PRETTY", "PREVENT", "PRICE", "PRIDE", "PRIMARY", "PRINT", "PRIORITY", "PRISON",
    "PRIVATE", "PRIZE", "PROBLEM", "PROCESS", "PRODUCE", "PROFIT", "PROGRAM", "PROJECT", "PROMOTE",
    "PROOF", "PROPERTY", "PROSPER", "PROTECT", "PROUD", "PROVIDE", "PUBLIC", "PUDDING", "PULL",
    "PULP", "PULSE", "PUMPKIN", "PUNCH", "PUPIL", "PUPPY", "PURCHASE", "PURITY", "PURPOSE",
    "PURSE", "PUSH", "PUT", "PUZZLE", "PYRAMID", "QUALITY", "QUANTUM", "QUARTER", "QUESTION",
    "QUICK", "QUIT", "QUIZ", "QUOTE", "RABBIT", "RACCOON", "RACE", "RACK", "RADAR", "RADIO",
    "RAIL", "RAIN", "RAISE", "RALLY", "RAMP", "RANCH", "RANDOM", "RANGE", "RAPID", "RARE", "RATE",
    "RATHER", "RAVEN", "RAW", "RAZOR", "READY", "REAL", "REASON", "REBEL", "REBUILD", "RECALL",
    "RECEIVE", "RECIPE", "RECORD", "RECYCLE", "REDUCE", "REFLECT", "REFORM", "REFUSE", "REGION",
    "REGRET", "REGULAR", "REJECT", "RELAX", "RELEASE", "RELIEF", "RELY", "REMAIN", "REMEMBER",
    "REMIND", "REMOVE", "RENDER", "RENEW", "RENT", "REOPEN", "REPAIR", "REPEAT", "REPLACE",
    "REPORT", "REQUIRE", "RESCUE", "RESEMBLE", "RESIST", "RESOURCE", "RESPONSE", "RESULT",
    "RETIRE", "RETREAT", "RETURN", "REUNION", "REVEAL", "REVIEW", "REWARD", "RHYTHM", "RIB",
    "RIBBON", "RICE", "RICH", "RIDE", "RIDGE", "RIFLE", "RIGHT", "RIGID", "RING", "RIOT", "RIPPLE",
    "RISK", "RITUAL", "RIVAL", "RIVER", "ROAD", "ROAST", "ROBOT", "ROBUST", "ROCKET", "ROMANCE",
    "ROOF", "ROOKIE", "ROOM", "ROSE", "ROTATE", "ROUGH", "ROUND", "ROUTE", "ROYAL", "RUBBER",
    "RUDE", "RUG", "RULE", "RUN", "RUNWAY", "RURAL", "SAD", "SADDLE", "SADNESS", "SAFE", "SAIL",
    "SALAD", "SALMON", "SALON", "SALT", "SALUTE", "SAME", "SAMPLE", "SAND", "SATISFY", "SATOSHI",
    "SAUCE", "SAUSAGE", "SAVE", "SAY", "SCALE", "SCAN", "SCARE", "SCATTER", "SCENE", "SCHEME",
    "SCHOOL", "SCIENCE", "SCISSORS", "SCORPION", "SCOUT", "SCRAP", "SCREEN", "SCRIPT", "SCRUB",
    "SEA", "SEARCH", "SEASON", "SEAT", "SECOND", "SECRET", "SECTION", "SECURITY", "SEED", "SEEK",
    "SEGMENT", "SELECT", "SELL", "SEMINAR", "SENIOR", "SENSE", "SENTENCE", "SERIES", "SERVICE",
    "SESSION", "SETTLE", "SETUP", "SEVEN", "SHADOW", "SHAFT", "SHALLOW", "SHARE", "SHED", "SHELL",
    "SHERIFF", "SHIELD", "SHIFT", "SHINE", "SHIP", "SHIVER", "SHOCK", "SHOE", "SHOOT", "SHOP",
    "SHORT", "SHOULDER", "SHOVE", "SHRIMP", "SHRUG", "SHUFFLE", "SHY", "SIBLING", "SICK", "SIDE",
    "SIEGE", "SIGHT", "SIGN", "SILENT", "SILK", "SILLY", "SILVER", "SIMILAR", "SIMPLE", "SINCE",
    "SING", "SIREN", "SISTER", "SITUATE", "SIX", "SIZE", "SKATE", "SKETCH", "SKI", "SKILL", "SKIN",
    "SKIRT", "SKULL", "SLAB", "SLAM", "SLEEP", "SLENDER", "SLICE", "SLIDE", "SLIGHT", "SLIM",
    "SLOGAN", "SLOT", "SLOW", "SLUSH", "SMALL", "SMART", "SMILE", "SMOKE", "SMOOTH", "SNACK",
    "SNAKE", "SNAP", "SNIFF", "SNOW", "SOAP", "SOCCER", "SOCIAL", "SOCK", "SODA", "SOFT", "SOLAR",
    "SOLDIER", "SOLID", "SOLUTION", "SOLVE", "SOMEONE", "SONG", "SOON", "SORRY", "SORT", "SOUL",
    "SOUND", "SOUP", "SOURCE", "SOUTH", "SPACE", "SPARE", "SPATIAL", "SPAWN", "SPEAK", "SPECIAL",
    "SPEED", "SPELL", "SPEND", "SPHERE", "SPICE", "SPIDER", "SPIKE", "SPIN", "SPIRIT", "SPLIT",
    "SPOIL", "SPONSOR", "SPOON", "SPORT", "SPOT", "SPRAY", "SPREAD", "SPRING", "SPY", "SQUARE",
    "SQUEEZE", "SQUIRREL", "STABLE", "STADIUM", "STAFF", "STAGE", "STAIRS", "STAMP", "STAND",
    "START", "STATE", "STAY", "STEAK", "STEEL", "STEM", "STEP", "STEREO", "STICK", "STILL",
    "STING", "STOCK", "STOMACH", "STONE", "STOOL", "STORY", "STOVE", "STRATEGY", "STREET",
    "STRIKE", "STRONG", "STRUGGLE", "STUDENT", "STUFF", "STUMBLE", "STYLE", "SUBJECT", "SUBMIT",
    "SUBWAY", "SUCCESS", "SUCH", "SUDDEN", "SUFFER", "SUGAR", "SUGGEST", "SUIT", "SUMMER", "SUN",
    "SUNNY", "SUNSET", "SUPER", "SUPPLY", "SUPREME", "SURE", "SURFACE", "SURGE", "SURPRISE",
    "SURROUND", "SURVEY", "SUSPECT", "SUSTAIN", "SWALLOW", "SWAMP", "SWAP", "SWARM", "SWEAR",
    "SWEET", "SWIFT", "SWIM", "SWING", "SWITCH", "SWORD", "SYMBOL", "SYMPTOM", "SYRUP", "SYSTEM",
    "TABLE", "TACKLE", "TAG", "TAIL", "TALENT", "TALK", "TANK", "TAPE", "TARGET", "TASK", "TASTE",
    "TATTOO", "TAXI", "TEACH", "TEAM", "TELL", "TEN", "TENANT", "TENNIS", "TENT", "TERM", "TEST",
    "TEXT", "THANK", "THAT", "THEME", "THEN", "THEORY", "THERE", "THEY", "THING", "THIS",
    "THOUGHT", "THREE", "THRIVE", "THROW", "THUMB", "THUNDER", "TICKET", "TIDE", "TIGER", "TILT",
    "TIMBER", "TIME", "TINY", "TIP", "TIRED", "TISSUE", "TITLE", "TOAST", "TOBACCO", "TODAY",
    "TODDLER", "TOE", "TOGETHER", "TOILET", "TOKEN", "TOMATO", "TOMORROW", "TONE", "TONGUE",
    "TONIGHT", "TOOL", "TOOTH", "TOP", "TOPIC", "TOPPLE", "TORCH", "TORNADO", "TORTOISE", "TOSS",
    "TOTAL", "TOURIST", "TOWARD", "TOWER", "TOWN", "TOY", "TRACK", "TRADE", "TRAFFIC", "TRAGIC",
    "TRAIN", "TRANSFER", "TRAP", "TRASH", "TRAVEL", "TRAY", "TREAT", "TREE", "TREND", "TRIAL",
    "TRIBE", "TRICK", "TRIGGER", "TRIM", "TRIP", "TROPHY", "TROUBLE", "TRUCK", "TRUE", "TRULY",
    "TRUMPET", "TRUST", "TRUTH", "TRY", "TUBE", "TUITION", "TUMBLE", "TUNA", "TUNNEL", "TURKEY",
    "TURN", "TURTLE", "TWELVE", "TWENTY", "TWICE", "TWIN", "TWIST", "TWO", "TYPE", "TYPICAL",
    "UGLY", "UMBRELLA", "UNABLE", "UNAWARE", "UNCLE", "UNCOVER", "UNDER", "UNDO", "UNFAIR",
    "UNFOLD", "UNHAPPY", "UNIFORM", "UNIQUE", "UNIT", "UNIVERSE", "UNKNOWN", "UNLOCK", "UNTIL",
    "UNUSUAL", "UNVEIL", "UPDATE", "UPGRADE", "UPHOLD", "UPON", "UPPER", "UPSET", "URBAN", "URGE",
    "USAGE", "USE", "USED", "USEFUL", "USELESS", "USUAL", "UTILITY", "VACANT", "VACUUM", "VAGUE",
    "VALID", "VALLEY", "VALVE", "VAN", "VANISH", "VAPOR", "VARIOUS", "VAST", "VAULT", "VEHICLE",
    "VELVET", "VENDOR", "VENTURE", "VENUE", "VERB", "VERIFY", "VERSION", "VERY", "VESSEL",
    "VETERAN", "VIABLE", "VIBRANT", "VICIOUS", "VICTORY", "VIDEO", "VIEW", "VILLAGE", "VINTAGE",
    "VIOLIN", "VIRTUAL", "VIRUS", "VISA", "VISIT", "VISUAL", "VITAL", "VIVID", "VOCAL", "VOICE",
    "VOID", "VOLCANO", "VOLUME", "VOTE", "VOYAGE", "WAGE", "WAGON", "WAIT", "WALK", "WALL",
    "WALNUT", "WANT", "WARFARE", "WARM", "WARRIOR", "WASH", "WASP", "WASTE", "WATER", "WAVE",
    "WAY", "WEALTH", "WEAPON", "WEAR", "WEASEL", "WEATHER", "WEB", "WEDDING", "WEEKEND", "WEIRD",
    "WELCOME", "WEST", "WET", "WHALE", "WHAT", "WHEAT", "WHEEL", "WHEN", "WHERE", "WHIP",
    "WHISPER", "WIDE", "WIDTH", "WIFE", "WILD", "WILL", "WIN", "WINDOW", "WINE", "WING", "WINK",
    "WINNER", "WINTER", "WIRE", "WISDOM", "WISE", "WISH", "WITNESS", "WOLF", "WOMAN", "WONDER",
    "WOOD", "WOOL", "WORD", "WORK", "WORLD", "WORRY", "WORTH", "WRAP", "WRECK", "WRESTLE", "WRIST",
    "WRITE", "WRONG", "YARD", "YEAR", "YELLOW", "YOU", "YOUNG", "YOUTH", "ZEBRA", "ZERO", "ZONE",
    "ZOO",
];

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::*;
    use alloc::{string::String, vec::Vec};
    use secp256kfun::hex;
    use sha2::{digest::Digest, Sha256};

    #[test]
    fn test_bip39_wordlist_hash() {
        // The official BIP39 English wordlist from:
        // https://github.com/bitcoin/bips/blob/master/bip-0039/english.txt
        // has this SHA256 hash when words are lowercase and joined with newlines
        const EXPECTED_HASH: &str =
            "2f5eed53a4727b4bf8880d8f3f199efc90e58503646d9ff8eff3a2ed3b24dbda";

        // Convert our uppercase words to lowercase and join with newlines
        let wordlist_string: String = BIP39_WORDS
            .iter()
            .map(|word| word.to_lowercase())
            .collect::<Vec<_>>()
            .join("\n");

        // Calculate SHA256 hash
        let mut hasher = Sha256::new();
        hasher.update(wordlist_string.as_bytes());
        hasher.update(b"\n");
        let result = hasher.finalize();
        let hash_hex = hex::encode(&result);

        assert_eq!(
            hash_hex, EXPECTED_HASH,
            "BIP39 wordlist hash mismatch! The wordlist may have been modified."
        );
    }
}
