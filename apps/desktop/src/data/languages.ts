export interface LanguageOption {
  code: string;
  name: string;
  /** ISO 3166-1 alpha-2 country code used to render a canonical flag.
   *  Many languages span many countries; we pick the most representative. */
  country: string;
}

/** All languages whisper.cpp can transcribe. Sorted alphabetically by name.
 *  `code` is the ISO tag passed through to `--language`. */
export const languages: LanguageOption[] = [
  { code: "af", name: "Afrikaans", country: "ZA" },
  { code: "sq", name: "Albanian", country: "AL" },
  { code: "am", name: "Amharic", country: "ET" },
  { code: "ar", name: "Arabic", country: "SA" },
  { code: "hy", name: "Armenian", country: "AM" },
  { code: "as", name: "Assamese", country: "IN" },
  { code: "az", name: "Azerbaijani", country: "AZ" },
  { code: "ba", name: "Bashkir", country: "RU" },
  { code: "eu", name: "Basque", country: "ES" },
  { code: "be", name: "Belarusian", country: "BY" },
  { code: "bn", name: "Bengali", country: "BD" },
  { code: "bs", name: "Bosnian", country: "BA" },
  { code: "br", name: "Breton", country: "FR" },
  { code: "bg", name: "Bulgarian", country: "BG" },
  { code: "my", name: "Burmese", country: "MM" },
  { code: "yue", name: "Cantonese", country: "HK" },
  { code: "ca", name: "Catalan", country: "ES" },
  { code: "zh", name: "Chinese", country: "CN" },
  { code: "hr", name: "Croatian", country: "HR" },
  { code: "cs", name: "Czech", country: "CZ" },
  { code: "da", name: "Danish", country: "DK" },
  { code: "nl", name: "Dutch", country: "NL" },
  { code: "en", name: "English", country: "GB" },
  { code: "et", name: "Estonian", country: "EE" },
  { code: "fo", name: "Faroese", country: "FO" },
  { code: "fi", name: "Finnish", country: "FI" },
  { code: "fr", name: "French", country: "FR" },
  { code: "gl", name: "Galician", country: "ES" },
  { code: "ka", name: "Georgian", country: "GE" },
  { code: "de", name: "German", country: "DE" },
  { code: "el", name: "Greek", country: "GR" },
  { code: "gu", name: "Gujarati", country: "IN" },
  { code: "ht", name: "Haitian Creole", country: "HT" },
  { code: "ha", name: "Hausa", country: "NG" },
  { code: "haw", name: "Hawaiian", country: "US" },
  { code: "he", name: "Hebrew", country: "IL" },
  { code: "hi", name: "Hindi", country: "IN" },
  { code: "hu", name: "Hungarian", country: "HU" },
  { code: "is", name: "Icelandic", country: "IS" },
  { code: "id", name: "Indonesian", country: "ID" },
  { code: "it", name: "Italian", country: "IT" },
  { code: "ja", name: "Japanese", country: "JP" },
  { code: "jw", name: "Javanese", country: "ID" },
  { code: "kn", name: "Kannada", country: "IN" },
  { code: "kk", name: "Kazakh", country: "KZ" },
  { code: "km", name: "Khmer", country: "KH" },
  { code: "ko", name: "Korean", country: "KR" },
  { code: "lo", name: "Lao", country: "LA" },
  { code: "la", name: "Latin", country: "VA" },
  { code: "lv", name: "Latvian", country: "LV" },
  { code: "ln", name: "Lingala", country: "CD" },
  { code: "lt", name: "Lithuanian", country: "LT" },
  { code: "lb", name: "Luxembourgish", country: "LU" },
  { code: "mk", name: "Macedonian", country: "MK" },
  { code: "mg", name: "Malagasy", country: "MG" },
  { code: "ms", name: "Malay", country: "MY" },
  { code: "ml", name: "Malayalam", country: "IN" },
  { code: "mt", name: "Maltese", country: "MT" },
  { code: "mi", name: "Maori", country: "NZ" },
  { code: "mr", name: "Marathi", country: "IN" },
  { code: "mn", name: "Mongolian", country: "MN" },
  { code: "ne", name: "Nepali", country: "NP" },
  { code: "no", name: "Norwegian", country: "NO" },
  { code: "nn", name: "Nynorsk", country: "NO" },
  { code: "oc", name: "Occitan", country: "FR" },
  { code: "ps", name: "Pashto", country: "AF" },
  { code: "fa", name: "Persian", country: "IR" },
  { code: "pl", name: "Polish", country: "PL" },
  { code: "pt", name: "Portuguese", country: "PT" },
  { code: "pa", name: "Punjabi", country: "IN" },
  { code: "ro", name: "Romanian", country: "RO" },
  { code: "ru", name: "Russian", country: "RU" },
  { code: "sa", name: "Sanskrit", country: "IN" },
  { code: "sr", name: "Serbian", country: "RS" },
  { code: "sn", name: "Shona", country: "ZW" },
  { code: "sd", name: "Sindhi", country: "PK" },
  { code: "si", name: "Sinhala", country: "LK" },
  { code: "sk", name: "Slovak", country: "SK" },
  { code: "sl", name: "Slovenian", country: "SI" },
  { code: "so", name: "Somali", country: "SO" },
  { code: "es", name: "Spanish", country: "ES" },
  { code: "su", name: "Sundanese", country: "ID" },
  { code: "sw", name: "Swahili", country: "TZ" },
  { code: "sv", name: "Swedish", country: "SE" },
  { code: "tl", name: "Tagalog", country: "PH" },
  { code: "tg", name: "Tajik", country: "TJ" },
  { code: "ta", name: "Tamil", country: "IN" },
  { code: "tt", name: "Tatar", country: "RU" },
  { code: "te", name: "Telugu", country: "IN" },
  { code: "th", name: "Thai", country: "TH" },
  { code: "bo", name: "Tibetan", country: "CN" },
  { code: "tr", name: "Turkish", country: "TR" },
  { code: "tk", name: "Turkmen", country: "TM" },
  { code: "uk", name: "Ukrainian", country: "UA" },
  { code: "ur", name: "Urdu", country: "PK" },
  { code: "uz", name: "Uzbek", country: "UZ" },
  { code: "vi", name: "Vietnamese", country: "VN" },
  { code: "cy", name: "Welsh", country: "GB" },
  { code: "yi", name: "Yiddish", country: "IL" },
  { code: "yo", name: "Yoruba", country: "NG" },
];

/** Convert an ISO 3166-1 alpha-2 country code to the regional-indicator emoji
 *  flag sequence. `US` → 🇺🇸. Combined with the Twemoji Country Flags webfont
 *  loaded in styles.css, this renders identically on Mac, Windows, and Linux. */
export function flag(country: string): string {
  const upper = country.toUpperCase();
  if (upper.length !== 2) return "";
  const codePoints = [...upper].map((ch) => 127397 + ch.charCodeAt(0));
  return String.fromCodePoint(...codePoints);
}

/** Whisper's own paper (Table 12) publishes per-language WER by model size.
 *  Below ~30% WER is roughly the point where dictation is usable. Tiers here
 *  approximate that curve so we can hide options the current model would
 *  hallucinate on rather than transcribe. */

/** `.en` models are trained on English speech only. */
export const englishOnly: readonly string[] = ["en"];

/** Languages where `tiny` / `base` multilingual reliably stay under ~30% WER. */
export const majorLanguages: readonly string[] = [
  "en", "es", "fr", "de", "it", "pt", "nl", "ru", "pl", "uk",
  "cs", "hu", "tr", "ja", "ko", "zh", "ca", "id", "ro", "sv",
  "da", "no", "fi",
];

/** Adds most South/South-East Asian, Middle-Eastern, Balkan, Baltic scripts.
 *  `small` handles these well enough for dictation. */
export const broadLanguages: readonly string[] = [
  ...majorLanguages,
  "ar", "he", "el", "bg", "sr", "hr", "sk", "sl", "lt", "lv",
  "et", "vi", "th", "ms", "tl", "hi", "bn", "ta", "ur", "fa",
  "be", "mk", "kk", "az", "ka", "hy", "is", "cy", "eu", "gl",
  "bs", "sq", "yue", "pa", "mr", "af", "mt", "te", "ml", "kn",
  "ne", "si", "km", "lo", "my",
];

/** Everything Whisper ships weights for. `medium` and `large-v3-turbo`. */
export const allLanguages: readonly string[] = languages.map((l) => l.code);
