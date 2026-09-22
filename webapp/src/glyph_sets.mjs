// The curated glyph sets behind Insert ▸ Symbol and Insert ▸ Emoji.
//
// Data, and only data: `glyph_picker.mjs` holds the dialog and `main.js` holds
// the insertion. They are here because 242 lines of literal arrays in an
// 18k-line file is 242 lines nothing can review in isolation and nothing can
// reuse — the same HF-085 argument that moved the picker itself out, one file
// smaller. Each entry carries `c` (the character) and `n` (its name), which is
// both the tooltip and what the picker's search matches on.
//
// Curated rather than generated from a Unicode database on purpose: the sets
// are what Word's Symbol dialog and Docs' special-character picker actually
// offer, which is a far shorter and far more useful list than "every character
// with this property".

export const SYMBOL_GROUPS = [
  {
    name: "Currency",
    items: [
      { c: "€", n: "Euro" }, { c: "£", n: "Pound sterling" }, { c: "¥", n: "Yen" },
      { c: "¢", n: "Cent" }, { c: "₹", n: "Indian rupee" }, { c: "₩", n: "Won" },
      { c: "₽", n: "Ruble" }, { c: "₺", n: "Turkish lira" }, { c: "₴", n: "Hryvnia" },
      { c: "₦", n: "Naira" }, { c: "฿", n: "Baht" }, { c: "₫", n: "Dong" },
      { c: "₪", n: "Shekel" }, { c: "₱", n: "Peso" }, { c: "$", n: "Dollar" }, { c: "¤", n: "Currency sign" },
    ],
  },
  {
    name: "Math",
    items: [
      { c: "±", n: "Plus-minus" }, { c: "×", n: "Multiplication" }, { c: "÷", n: "Division" },
      { c: "≠", n: "Not equal to" }, { c: "≤", n: "Less than or equal" }, { c: "≥", n: "Greater than or equal" },
      { c: "≈", n: "Almost equal to" }, { c: "≡", n: "Identical to" }, { c: "∞", n: "Infinity" },
      { c: "∑", n: "Summation" }, { c: "∏", n: "Product" }, { c: "√", n: "Square root" },
      { c: "∫", n: "Integral" }, { c: "∂", n: "Partial differential" }, { c: "∆", n: "Increment (delta)" },
      { c: "∇", n: "Nabla" }, { c: "∈", n: "Element of" }, { c: "∉", n: "Not an element of" },
      { c: "∅", n: "Empty set" }, { c: "∩", n: "Intersection" }, { c: "∪", n: "Union" },
      { c: "⊂", n: "Subset of" }, { c: "⊃", n: "Superset of" }, { c: "°", n: "Degree" },
      { c: "µ", n: "Micro sign" }, { c: "∝", n: "Proportional to" }, { c: "∴", n: "Therefore" },
      { c: "∵", n: "Because" }, { c: "¬", n: "Not sign" }, { c: "∧", n: "Logical and" },
      { c: "∨", n: "Logical or" }, { c: "‰", n: "Per mille" }, { c: "′", n: "Prime" }, { c: "″", n: "Double prime" },
    ],
  },
  {
    name: "Arrows",
    items: [
      { c: "←", n: "Leftwards arrow" }, { c: "→", n: "Rightwards arrow" }, { c: "↑", n: "Upwards arrow" },
      { c: "↓", n: "Downwards arrow" }, { c: "↔", n: "Left-right arrow" }, { c: "↕", n: "Up-down arrow" },
      { c: "↖", n: "Up-left arrow" }, { c: "↗", n: "Up-right arrow" }, { c: "↘", n: "Down-right arrow" },
      { c: "↙", n: "Down-left arrow" }, { c: "⇐", n: "Leftwards double arrow" }, { c: "⇒", n: "Rightwards double arrow" },
      { c: "⇑", n: "Upwards double arrow" }, { c: "⇓", n: "Downwards double arrow" }, { c: "⇔", n: "Left-right double arrow" },
      { c: "↩", n: "Leftwards arrow with hook" }, { c: "↪", n: "Rightwards arrow with hook" }, { c: "⟶", n: "Long rightwards arrow" },
      { c: "➔", n: "Heavy round-tipped rightwards arrow" }, { c: "↺", n: "Anticlockwise open circle arrow" }, { c: "↻", n: "Clockwise open circle arrow" },
    ],
  },
  {
    name: "Punctuation",
    items: [
      { c: "–", n: "En dash" }, { c: "—", n: "Em dash" }, { c: "…", n: "Horizontal ellipsis" },
      { c: "‘", n: "Left single quotation mark" }, { c: "’", n: "Right single quotation mark" },
      { c: "“", n: "Left double quotation mark" }, { c: "”", n: "Right double quotation mark" },
      { c: "‚", n: "Single low-9 quotation mark" }, { c: "„", n: "Double low-9 quotation mark" },
      { c: "«", n: "Left-pointing double angle quotation" }, { c: "»", n: "Right-pointing double angle quotation" },
      { c: "‹", n: "Single left-pointing angle quotation" }, { c: "›", n: "Single right-pointing angle quotation" },
      { c: "†", n: "Dagger" }, { c: "‡", n: "Double dagger" }, { c: "§", n: "Section sign" },
      { c: "¶", n: "Pilcrow (paragraph)" }, { c: "•", n: "Bullet" }, { c: "·", n: "Middle dot" },
      { c: "※", n: "Reference mark" }, { c: "¡", n: "Inverted exclamation mark" }, { c: "¿", n: "Inverted question mark" },
    ],
  },
  {
    name: "Latin",
    items: [
      { c: "á", n: "a acute" }, { c: "à", n: "a grave" }, { c: "â", n: "a circumflex" }, { c: "ä", n: "a diaeresis" },
      { c: "ã", n: "a tilde" }, { c: "å", n: "a ring" }, { c: "é", n: "e acute" }, { c: "è", n: "e grave" },
      { c: "ê", n: "e circumflex" }, { c: "ë", n: "e diaeresis" }, { c: "í", n: "i acute" }, { c: "î", n: "i circumflex" },
      { c: "ï", n: "i diaeresis" }, { c: "ó", n: "o acute" }, { c: "ô", n: "o circumflex" }, { c: "ö", n: "o diaeresis" },
      { c: "õ", n: "o tilde" }, { c: "ø", n: "o stroke" }, { c: "ú", n: "u acute" }, { c: "û", n: "u circumflex" },
      { c: "ü", n: "u diaeresis" }, { c: "ñ", n: "n tilde" }, { c: "ç", n: "c cedilla" }, { c: "ß", n: "Sharp s (eszett)" },
      { c: "æ", n: "ae ligature" }, { c: "œ", n: "oe ligature" }, { c: "ā", n: "a macron" }, { c: "ē", n: "e macron" },
      { c: "ī", n: "i macron" }, { c: "ō", n: "o macron" }, { c: "ū", n: "u macron" }, { c: "ý", n: "y acute" },
    ],
  },
  {
    name: "Greek",
    items: [
      { c: "α", n: "alpha" }, { c: "β", n: "beta" }, { c: "γ", n: "gamma" }, { c: "δ", n: "delta" },
      { c: "ε", n: "epsilon" }, { c: "ζ", n: "zeta" }, { c: "η", n: "eta" }, { c: "θ", n: "theta" },
      { c: "ι", n: "iota" }, { c: "κ", n: "kappa" }, { c: "λ", n: "lambda" }, { c: "μ", n: "mu" },
      { c: "ν", n: "nu" }, { c: "ξ", n: "xi" }, { c: "π", n: "pi" }, { c: "ρ", n: "rho" },
      { c: "σ", n: "sigma" }, { c: "τ", n: "tau" }, { c: "φ", n: "phi" }, { c: "χ", n: "chi" },
      { c: "ψ", n: "psi" }, { c: "ω", n: "omega" }, { c: "Γ", n: "Gamma capital" }, { c: "Δ", n: "Delta capital" },
      { c: "Θ", n: "Theta capital" }, { c: "Λ", n: "Lambda capital" }, { c: "Ξ", n: "Xi capital" }, { c: "Π", n: "Pi capital" },
      { c: "Σ", n: "Sigma capital" }, { c: "Φ", n: "Phi capital" }, { c: "Ψ", n: "Psi capital" }, { c: "Ω", n: "Omega capital" },
    ],
  },
  {
    name: "Fractions",
    items: [
      { c: "½", n: "One half" }, { c: "⅓", n: "One third" }, { c: "⅔", n: "Two thirds" }, { c: "¼", n: "One quarter" },
      { c: "¾", n: "Three quarters" }, { c: "⅕", n: "One fifth" }, { c: "⅖", n: "Two fifths" }, { c: "⅗", n: "Three fifths" },
      { c: "⅘", n: "Four fifths" }, { c: "⅙", n: "One sixth" }, { c: "⅚", n: "Five sixths" }, { c: "⅛", n: "One eighth" },
      { c: "⅜", n: "Three eighths" }, { c: "⅝", n: "Five eighths" }, { c: "⅞", n: "Seven eighths" }, { c: "№", n: "Numero sign" },
      { c: "™", n: "Trade mark" }, { c: "©", n: "Copyright" }, { c: "®", n: "Registered" }, { c: "℅", n: "Care of" }, { c: "ℓ", n: "Script small l (litre)" },
    ],
  },
];

export const EMOJI_GROUPS = [
  {
    name: "Smileys",
    icon: "😀",
    items: [
      { c: "😀", n: "grinning face" }, { c: "😃", n: "smiley open mouth" }, { c: "😄", n: "smiling eyes" },
      { c: "😁", n: "beaming grin" }, { c: "😆", n: "laughing squint" }, { c: "😅", n: "sweat smile" },
      { c: "😂", n: "tears of joy laughing" }, { c: "🤣", n: "rolling on floor laughing" }, { c: "😊", n: "blush smiling" },
      { c: "🙂", n: "slight smile" }, { c: "🙃", n: "upside down" }, { c: "😉", n: "wink" },
      { c: "😌", n: "relieved" }, { c: "😍", n: "heart eyes love" }, { c: "🥰", n: "smiling hearts love" },
      { c: "😘", n: "blowing kiss" }, { c: "😗", n: "kissing" }, { c: "😋", n: "yum savoring" },
      { c: "😛", n: "tongue out" }, { c: "😜", n: "wink tongue" }, { c: "🤪", n: "zany goofy" },
      { c: "🤨", n: "raised eyebrow skeptical" }, { c: "😐", n: "neutral face" }, { c: "😑", n: "expressionless" },
      { c: "😶", n: "no mouth" }, { c: "🙄", n: "rolling eyes" }, { c: "😏", n: "smirk" },
      { c: "😴", n: "sleeping" }, { c: "😷", n: "mask sick" }, { c: "🤒", n: "thermometer ill" },
      { c: "🤗", n: "hugging" }, { c: "🤔", n: "thinking" }, { c: "🤯", n: "mind blown exploding head" },
      { c: "🥳", n: "party face celebrate" }, { c: "😎", n: "sunglasses cool" }, { c: "🤓", n: "nerd" },
      { c: "😕", n: "confused" }, { c: "🙁", n: "slight frown" }, { c: "😢", n: "crying" },
      { c: "😭", n: "sobbing loud cry" }, { c: "😱", n: "screaming fear" }, { c: "😳", n: "flushed" },
      { c: "🥺", n: "pleading puppy eyes" }, { c: "😡", n: "angry pouting" }, { c: "😠", n: "angry" },
    ],
  },
  {
    name: "People",
    icon: "👋",
    items: [
      { c: "👋", n: "waving hand hello" }, { c: "🤚", n: "raised back of hand" }, { c: "✋", n: "raised hand stop" },
      { c: "👌", n: "ok hand" }, { c: "🤏", n: "pinching hand small" }, { c: "✌️", n: "victory peace" },
      { c: "🤞", n: "crossed fingers luck" }, { c: "🤟", n: "love you gesture" }, { c: "🤘", n: "rock on horns" },
      { c: "🤙", n: "call me hand" }, { c: "👈", n: "backhand pointing left" }, { c: "👉", n: "backhand pointing right" },
      { c: "👆", n: "backhand pointing up" }, { c: "👇", n: "backhand pointing down" }, { c: "☝️", n: "index pointing up" },
      { c: "👍", n: "thumbs up like" }, { c: "👎", n: "thumbs down dislike" }, { c: "✊", n: "raised fist" },
      { c: "👊", n: "fist bump" }, { c: "👏", n: "clapping applause" }, { c: "🙌", n: "raising hands celebrate" },
      { c: "🙏", n: "folded hands thanks pray" }, { c: "💪", n: "flexed biceps strong" }, { c: "👀", n: "eyes looking" },
      { c: "👶", n: "baby" }, { c: "🧑", n: "person adult" }, { c: "👨", n: "man" },
      { c: "👩", n: "woman" }, { c: "👴", n: "old man" }, { c: "👵", n: "old woman" },
    ],
  },
  {
    name: "Nature",
    icon: "🐻",
    items: [
      { c: "🐶", n: "dog face" }, { c: "🐱", n: "cat face" }, { c: "🐭", n: "mouse" }, { c: "🐹", n: "hamster" },
      { c: "🐰", n: "rabbit" }, { c: "🦊", n: "fox" }, { c: "🐻", n: "bear" }, { c: "🐼", n: "panda" },
      { c: "🐨", n: "koala" }, { c: "🐯", n: "tiger face" }, { c: "🦁", n: "lion" }, { c: "🐮", n: "cow face" },
      { c: "🐷", n: "pig face" }, { c: "🐸", n: "frog" }, { c: "🐵", n: "monkey face" }, { c: "🐔", n: "chicken" },
      { c: "🐧", n: "penguin" }, { c: "🐦", n: "bird" }, { c: "🦆", n: "duck" }, { c: "🦉", n: "owl" },
      { c: "🐴", n: "horse face" }, { c: "🦄", n: "unicorn" }, { c: "🐝", n: "bee honeybee" }, { c: "🦋", n: "butterfly" },
      { c: "🐌", n: "snail" }, { c: "🐞", n: "lady beetle bug" }, { c: "🐢", n: "turtle" }, { c: "🐍", n: "snake" },
      { c: "🐙", n: "octopus" }, { c: "🐠", n: "tropical fish" }, { c: "🐬", n: "dolphin" }, { c: "🐳", n: "whale" },
      { c: "🌵", n: "cactus" }, { c: "🌲", n: "evergreen tree" }, { c: "🌳", n: "deciduous tree" }, { c: "🌴", n: "palm tree" },
      { c: "🍀", n: "four leaf clover luck" }, { c: "🍁", n: "maple leaf" }, { c: "🌸", n: "cherry blossom" }, { c: "🌻", n: "sunflower" },
      { c: "🌹", n: "rose" }, { c: "🌷", n: "tulip" }, { c: "🌼", n: "blossom flower" }, { c: "🍄", n: "mushroom" },
    ],
  },
  {
    name: "Food",
    icon: "🍎",
    items: [
      { c: "🍎", n: "red apple" }, { c: "🍐", n: "pear" }, { c: "🍊", n: "tangerine orange" }, { c: "🍋", n: "lemon" },
      { c: "🍌", n: "banana" }, { c: "🍉", n: "watermelon" }, { c: "🍇", n: "grapes" }, { c: "🍓", n: "strawberry" },
      { c: "🍒", n: "cherries" }, { c: "🍑", n: "peach" }, { c: "🥭", n: "mango" }, { c: "🍍", n: "pineapple" },
      { c: "🥝", n: "kiwi" }, { c: "🍅", n: "tomato" }, { c: "🥑", n: "avocado" }, { c: "🥦", n: "broccoli" },
      { c: "🥕", n: "carrot" }, { c: "🌽", n: "corn" }, { c: "🥔", n: "potato" }, { c: "🍞", n: "bread" },
      { c: "🧀", n: "cheese" }, { c: "🥚", n: "egg" }, { c: "🍳", n: "fried egg cooking" }, { c: "🥞", n: "pancakes" },
      { c: "🍔", n: "hamburger" }, { c: "🍟", n: "french fries" }, { c: "🍕", n: "pizza" }, { c: "🌭", n: "hot dog" },
      { c: "🌮", n: "taco" }, { c: "🌯", n: "burrito" }, { c: "🥗", n: "green salad" }, { c: "🍜", n: "steaming noodle bowl ramen" },
      { c: "🍝", n: "spaghetti pasta" }, { c: "🍣", n: "sushi" }, { c: "🍦", n: "soft ice cream" }, { c: "🍰", n: "shortcake slice" },
      { c: "🎂", n: "birthday cake" }, { c: "🍫", n: "chocolate bar" }, { c: "🍬", n: "candy" }, { c: "🍭", n: "lollipop" },
      { c: "☕", n: "hot coffee tea" }, { c: "🍵", n: "teacup" }, { c: "🍺", n: "beer mug" }, { c: "🍷", n: "wine glass" },
    ],
  },
  {
    name: "Activity",
    icon: "⚽",
    items: [
      { c: "⚽", n: "soccer football" }, { c: "🏀", n: "basketball" }, { c: "🏈", n: "american football" }, { c: "⚾", n: "baseball" },
      { c: "🎾", n: "tennis" }, { c: "🏐", n: "volleyball" }, { c: "🏉", n: "rugby" }, { c: "🎱", n: "pool 8 ball billiards" },
      { c: "🏓", n: "ping pong table tennis" }, { c: "🏸", n: "badminton" }, { c: "🥅", n: "goal net" }, { c: "🏒", n: "ice hockey" },
      { c: "🏑", n: "field hockey" }, { c: "🏏", n: "cricket" }, { c: "⛳", n: "flag in hole golf" }, { c: "🏹", n: "bow and arrow archery" },
      { c: "🎣", n: "fishing pole" }, { c: "🥊", n: "boxing glove" }, { c: "🥋", n: "martial arts uniform" }, { c: "⛸️", n: "ice skate" },
      { c: "🎿", n: "skis" }, { c: "🏂", n: "snowboarder" }, { c: "🏋️", n: "weight lifter" }, { c: "🤸", n: "cartwheel gymnast" },
      { c: "🏄", n: "surfer" }, { c: "🏊", n: "swimmer" }, { c: "🚴", n: "cyclist bicycle" }, { c: "🎮", n: "video game controller" },
      { c: "🎲", n: "game die dice" }, { c: "🎯", n: "direct hit bullseye dart" }, { c: "🎳", n: "bowling" }, { c: "🎸", n: "guitar" },
      { c: "🎹", n: "musical keyboard piano" }, { c: "🥁", n: "drum" }, { c: "🎺", n: "trumpet" }, { c: "🎻", n: "violin" },
      { c: "🎬", n: "clapper board movie" }, { c: "🎨", n: "artist palette paint" }, { c: "🎤", n: "microphone sing" }, { c: "🎧", n: "headphones" },
    ],
  },
  {
    name: "Travel",
    icon: "✈️",
    items: [
      { c: "🚗", n: "car automobile" }, { c: "🚕", n: "taxi" }, { c: "🚙", n: "sport utility vehicle" }, { c: "🚌", n: "bus" },
      { c: "🏎️", n: "racing car" }, { c: "🚓", n: "police car" }, { c: "🚑", n: "ambulance" }, { c: "🚒", n: "fire engine" },
      { c: "🚚", n: "delivery truck" }, { c: "🚜", n: "tractor" }, { c: "🚲", n: "bicycle" }, { c: "🛵", n: "motor scooter" },
      { c: "🏍️", n: "motorcycle" }, { c: "🚨", n: "police light siren" }, { c: "✈️", n: "airplane" }, { c: "🚀", n: "rocket" },
      { c: "🛸", n: "flying saucer ufo" }, { c: "🚁", n: "helicopter" }, { c: "⛵", n: "sailboat" }, { c: "🚤", n: "speedboat" },
      { c: "⚓", n: "anchor" }, { c: "🚉", n: "station train" }, { c: "🚄", n: "high speed train" }, { c: "🚇", n: "metro subway" },
      { c: "🗺️", n: "world map" }, { c: "🗽", n: "statue of liberty" }, { c: "🗼", n: "tokyo tower" }, { c: "🏰", n: "castle" },
      { c: "🎡", n: "ferris wheel" }, { c: "🎢", n: "roller coaster" }, { c: "⛲", n: "fountain" }, { c: "🏖️", n: "beach with umbrella" },
      { c: "🏝️", n: "desert island" }, { c: "🏔️", n: "snow-capped mountain" }, { c: "🌋", n: "volcano" }, { c: "🏕️", n: "camping" },
      { c: "⛺", n: "tent" }, { c: "🏠", n: "house home" }, { c: "🏡", n: "house with garden" }, { c: "🌆", n: "cityscape at dusk" },
    ],
  },
  {
    name: "Objects",
    icon: "💡",
    items: [
      { c: "⌚", n: "watch" }, { c: "📱", n: "mobile phone" }, { c: "💻", n: "laptop computer" }, { c: "⌨️", n: "keyboard" },
      { c: "🖥️", n: "desktop computer" }, { c: "🖨️", n: "printer" }, { c: "🖱️", n: "computer mouse" }, { c: "💾", n: "floppy disk save" },
      { c: "💿", n: "optical disc cd" }, { c: "📷", n: "camera" }, { c: "📹", n: "video camera" }, { c: "🎥", n: "movie camera" },
      { c: "📞", n: "telephone receiver" }, { c: "☎️", n: "telephone" }, { c: "📺", n: "television" }, { c: "📻", n: "radio" },
      { c: "🔋", n: "battery" }, { c: "🔌", n: "electric plug" }, { c: "💡", n: "light bulb idea" }, { c: "🔦", n: "flashlight" },
      { c: "🕯️", n: "candle" }, { c: "💰", n: "money bag" }, { c: "💳", n: "credit card" }, { c: "🔧", n: "wrench" },
      { c: "🔨", n: "hammer" }, { c: "⚙️", n: "gear settings" }, { c: "🔩", n: "nut and bolt" }, { c: "⚖️", n: "balance scale" },
      { c: "🔗", n: "link chain" }, { c: "🔒", n: "locked" }, { c: "🔑", n: "key" }, { c: "🗝️", n: "old key" },
      { c: "🚪", n: "door" }, { c: "🧳", n: "luggage suitcase" }, { c: "⏰", n: "alarm clock" }, { c: "⌛", n: "hourglass done" },
      { c: "📦", n: "package box" }, { c: "✏️", n: "pencil" }, { c: "📌", n: "pushpin" }, { c: "📎", n: "paperclip" },
      { c: "🔍", n: "magnifying glass search" }, { c: "📖", n: "open book" }, { c: "📝", n: "memo note" }, { c: "📅", n: "calendar" },
    ],
  },
  {
    name: "Symbols",
    icon: "❤️",
    items: [
      { c: "❤️", n: "red heart love" }, { c: "🧡", n: "orange heart" }, { c: "💛", n: "yellow heart" }, { c: "💚", n: "green heart" },
      { c: "💙", n: "blue heart" }, { c: "💜", n: "purple heart" }, { c: "🖤", n: "black heart" }, { c: "🤍", n: "white heart" },
      { c: "💔", n: "broken heart" }, { c: "❣️", n: "heart exclamation" }, { c: "💕", n: "two hearts" }, { c: "💖", n: "sparkling heart" },
      { c: "💯", n: "hundred points perfect" }, { c: "⭐", n: "star" }, { c: "🌟", n: "glowing star" }, { c: "✨", n: "sparkles" },
      { c: "⚡", n: "high voltage lightning" }, { c: "🔥", n: "fire flame lit" }, { c: "💧", n: "droplet water" }, { c: "✅", n: "check mark button" },
      { c: "❌", n: "cross mark wrong" }, { c: "❗", n: "exclamation mark" }, { c: "❓", n: "question mark" }, { c: "⚠️", n: "warning" },
      { c: "♻️", n: "recycling" }, { c: "✔️", n: "check mark" }, { c: "➕", n: "plus" }, { c: "➖", n: "minus" },
      { c: "✖️", n: "multiply" }, { c: "➗", n: "divide" }, { c: "🔴", n: "red circle" }, { c: "🟠", n: "orange circle" },
      { c: "🟡", n: "yellow circle" }, { c: "🟢", n: "green circle" }, { c: "🔵", n: "blue circle" }, { c: "🟣", n: "purple circle" },
      { c: "⚫", n: "black circle" }, { c: "⚪", n: "white circle" }, { c: "🔶", n: "large orange diamond" }, { c: "🔷", n: "large blue diamond" },
    ],
  },
  {
    name: "Flags",
    icon: "🏁",
    items: [
      { c: "🏁", n: "chequered flag finish" }, { c: "🚩", n: "triangular flag" }, { c: "🎌", n: "crossed flags" }, { c: "🏴", n: "black flag" },
      { c: "🏳️", n: "white flag" }, { c: "🏳️‍🌈", n: "rainbow pride flag" }, { c: "🏴‍☠️", n: "pirate flag" }, { c: "🇺🇸", n: "United States flag" },
      { c: "🇬🇧", n: "United Kingdom flag" }, { c: "🇨🇦", n: "Canada flag" }, { c: "🇫🇷", n: "France flag" }, { c: "🇩🇪", n: "Germany flag" },
      { c: "🇮🇹", n: "Italy flag" }, { c: "🇪🇸", n: "Spain flag" }, { c: "🇯🇵", n: "Japan flag" }, { c: "🇨🇳", n: "China flag" },
      { c: "🇰🇷", n: "South Korea flag" }, { c: "🇮🇳", n: "India flag" }, { c: "🇧🇷", n: "Brazil flag" }, { c: "🇷🇺", n: "Russia flag" },
      { c: "🇲🇽", n: "Mexico flag" }, { c: "🇦🇺", n: "Australia flag" }, { c: "🇳🇱", n: "Netherlands flag" }, { c: "🇸🇪", n: "Sweden flag" },
      { c: "🇨🇭", n: "Switzerland flag" }, { c: "🇸🇬", n: "Singapore flag" }, { c: "🇦🇪", n: "United Arab Emirates flag" }, { c: "🇿🇦", n: "South Africa flag" },
    ],
  },
];
