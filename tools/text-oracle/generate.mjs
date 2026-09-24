// Parity fixtures for text wrapping, produced by Excalidraw's OWN textWrapping.ts.
//
//   EXCALIDRAW_DIR=/path/to/excalidraw [ORACLE_SHA=<sha>] \
//     node --import ./register.mjs generate.mjs [seed] [count]
//
// Runs the oracle's textWrapping.ts and textMeasurements.ts unmodified (see hooks.mjs),
// with a deterministic width model installed through the oracle's own hook
// `setCustomTextMetricsProvider` (textMeasurements.ts:113) — no canvas, no fonts, no DOM.
// Writes, relative to this directory:
//
//   ../../crates/draw-engine/tests/fixtures/text-wrap.oracle.json   replayed by
//       tests/ci_text_wrap_oracle.rs: wrap cases under three width models, the offsets
//       getWrappedTextLines reports, and parseTokens output for every hard line.
//   ../../crates/draw-engine/src/text/unicode.rs   the Unicode classes the oracle's regexes
//       use, evaluated by the same regex engine the oracle runs on, as range tables.
//
// Width models, all exact in f64 (every width is a multiple of 1/4):
//   jsdom : 10 per UTF-16 code unit — what the oracle's own unit tests run under
//           (jest-canvas-mock returns text.length, textMeasurements.ts:144 multiplies by 10).
//   table : additive per-code-point widths (proportional latin, fullwidth CJK, 0-width marks).
//   kern  : `table` plus pair kerning on whole-line measures ONLY. The oracle measures
//           candidate lines with getLineWidth (kerned) but single code units, broken words
//           and trailing whitespace with charWidth (one char, unkerned): a port that
//           measures the wrong thing at the wrong place fails this model.
//
// Before anything is written, the oracle's own unit expectations are replayed
// (packages/element/tests/textWrapping.test.ts); any mismatch exits 2 and writes nothing.
// Generate inside mcr.microsoft.com/playwright:v1.63.0-noble (the root's
// `make oracle-fixtures` does): the Unicode tables follow the Node that runs this.

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const CRATE = join(HERE, "..", "..", "crates", "draw-engine");
const FIXTURE = join(CRATE, "tests", "fixtures", "text-wrap.oracle.json");
const UNICODE_RS = join(CRATE, "src", "text", "unicode.rs");

const fail = (message, code = 1) => {
  console.error(`[text-oracle] ${message}`);
  process.exit(code);
};

// ---------------------------------------------------------------- the pinned oracle
const DIR = process.env.EXCALIDRAW_DIR || fail("EXCALIDRAW_DIR is not set");
const previousPin = existsSync(FIXTURE)
  ? JSON.parse(readFileSync(FIXTURE, "utf8")).oracle?.excalidraw
  : undefined;
// A moved pin is a deliberate act: say so with ORACLE_SHA, or regenerate at the old one.
const PIN = process.env.ORACLE_SHA || previousPin || fail("no pin: set ORACLE_SHA");
const HEAD = execFileSync("git", ["-c", "safe.directory=*", "-C", DIR, "rev-parse", "HEAD"], {
  encoding: "utf8",
}).trim();
if (HEAD !== PIN) fail(`${DIR} is at ${HEAD}, the pin is ${PIN}`);

const ORACLE_FILES = [
  "packages/element/src/textWrapping.ts",
  "packages/element/src/textMeasurements.ts",
];
const W = await import(pathToFileURL(join(DIR, ORACLE_FILES[0])).href);
const M = await import(pathToFileURL(join(DIR, ORACLE_FILES[1])).href);

const SEED = Number(process.argv[2] ?? 42);
const COUNT = Number(process.argv[3] ?? 3000);

// ---------------------------------------------------------------- width models
const cps = (s) => Array.from(s).map((c) => c.codePointAt(0));

// Astral widths depend on the HIGH SURROGATE only. charWidth caches by
// `char.charCodeAt(0)` (textMeasurements.ts:183), so astral chars sharing a high surrogate
// share one cached width in the oracle; keeping the model constant per surrogate stops that
// quirk from making a fixture depend on case order. The port caches per full char.
const ASTRAL_BY_HI = { 0xd83c: 24, 0xd83d: 24, 0xd83e: 24, 0xdb40: 0, 0xd835: 12, 0xd840: 20 };
const tableWidth = (cp) => {
  if (cp > 0xffff) return ASTRAL_BY_HI[0xd800 + ((cp - 0x10000) >> 10)] ?? 16;
  const ch = String.fromCodePoint(cp);
  if (cp === 0x20) return 5.5;
  if (cp === 0x09) return 11;
  if ([0x0d, 0x85, 0x200b, 0x200d, 0xfeff, 0xfe0f, 0x20e3].includes(cp)) return 0;
  if (cp === 0xa0 || cp === 0x202f) return 5.5;
  if (cp >= 0x2000 && cp <= 0x200a) return 7.25;
  if (cp === 0x3000) return 20;
  if ((cp >= 0x300 && cp <= 0x36f) || cp === 0x3099 || cp === 0x309a) return 0; // combining
  if ("ijl.,:;'|!".includes(ch)) return 5;
  if ("frt()[]{}".includes(ch)) return 7.5;
  if ("mw".includes(ch)) return 15;
  if ("MW".includes(ch)) return 17;
  if (cp >= 0x30 && cp <= 0x39) return 11;
  if (cp >= 0x61 && cp <= 0x7a) return 10.25;
  if (cp >= 0x41 && cp <= 0x5a) return 13.25;
  if (cp >= 0x21 && cp <= 0x7e) return 8.5;
  if (cp >= 0xc0 && cp <= 0x24f) return 10.25;
  if (cp >= 0x1100 && cp <= 0x11ff) return 20; // conjoining jamo
  if (cp >= 0xff61 && cp <= 0xff9f) return 10; // halfwidth katakana
  if (
    (cp >= 0x3000 && cp <= 0x30ff) ||
    (cp >= 0x3400 && cp <= 0x9fff) ||
    (cp >= 0xac00 && cp <= 0xd7af) ||
    (cp >= 0xff00 && cp <= 0xff60) ||
    (cp >= 0xffe0 && cp <= 0xffe6)
  )
    return 20;
  if (cp >= 0x2600 && cp <= 0x27bf) return 18;
  return 12;
};
const KERN = { AV: -1.75, To: -1.5, Wa: -1, ff: -0.5, "r.": -1.25, "  ": 0.5, "こん": -2, "(a": -0.75 };
const kernOf = (s) => {
  const a = Array.from(s);
  let k = 0;
  for (let i = 1; i < a.length; i++) k += KERN[a[i - 1] + a[i]] ?? 0;
  return k;
};
const MODELS = {
  jsdom: (s) => s.length * 10,
  table: (s) => cps(s).reduce((w, cp) => w + tableWidth(cp), 0),
  kern: (s) => cps(s).reduce((w, cp) => w + tableWidth(cp), 0) + kernOf(s),
};
let active = "table";
M.setCustomTextMetricsProvider({ getLineWidth: (text) => MODELS[active](text) });
const fontOf = (model) => `20px parity-${model}`;
const select = (model) => {
  active = model;
  M.charWidth.clearCache(fontOf(model));
};
const wrap = (model, text, maxWidth) => {
  select(model);
  return W.wrapText(text, fontOf(model), maxWidth);
};
const offsets = (model, text, maxWidth) => {
  select(model);
  return W.getWrappedTextLines(text, fontOf(model), maxWidth).map((l) => [l.start, l.end]);
};

// ---------------------------------------------------------------- self-check
// Verbatim from packages/element/tests/textWrapping.test.ts (jsdom model). If the hooks,
// the shim or the provider drifted from the real oracle, this fails before any fixture is
// written.
const ORACLE_UNIT = [
  ["Hello Excalidraw", 100, "Hello\nExcalidraw"],
  ["Hello😀", 10, "H\ne\nl\nl\no\n😀"],
  ["don't wrap this number 99,100.99", 300, "don't wrap this number\n99,100.99"],
  ["Hello     ", 50, "Hello"],
  ["Hello     ", 60, "Hello "],
  ["  Hello  World", 90, "  Hello\nWorld"],
  ["   Hello  World            ", 90, "   Hello\nWorld    "],
  ["Hello   Wo rl  d                     ", 100, "Hello   Wo\nrl  d     "],
  ["😀🗺🔥👩🏽‍🦰👨‍👩‍👧‍👦🇨🇿", 1, "😀\n🗺\n🔥\n👩🏽‍🦰\n👨‍👩‍👧‍👦\n🇨🇿"],
  ["Wikipedia is hosted by Wikimedia- Foundation, a non-profit organization that also hosts a range-of other projects", 110,
    "Wikipedia\nis hosted\nby\nWikimedia-\nFoundation,\na non-\nprofit\norganizatio\nn that also\nhosts a\nrange-of\nother\nprojects"],
  ["Hello thereusing-now", 100, "Hello\nthereusing\n-now"],
  ["\tA) one tab\t\t- two tabs        - 8 spaces", 100, "\tA) one\ntab\t\t- two\ntabs\n- 8 spaces"],
  ["\tA) one tab\t\t- two tabs        - 8 spaces", 50, "\tA)\none\ntab\n- two\ntabs\n- 8\nspace\ns"],
  ["안녕하세요こんにちは世界ｺﾝﾆﾁハ你好", 10, "안\n녕\n하\n세\n요\nこ\nん\nに\nち\nは\n世\n界\nｺ\nﾝ\nﾆ\nﾁ\nハ\n你\n好"],
  ["안녕하세요こんにちは世界ｺﾝﾆﾁハ你好", 30, "안녕하\n세요こ\nんにち\nは世界\nｺﾝﾆ\nﾁハ你\n好"],
  ["a醫 醫      bb  你好  world-i-😀🗺🔥", 150, "a醫 醫      bb  你\n好  world-i-😀🗺\n🔥"],
  ["a醫 醫      bb  你好  world-i-😀🗺🔥", 50, "a醫 醫\nbb  你\n好\nworld\n-i-😀\n🗺🔥"],
  ["a醫 醫      bb  你好  world-i-😀🗺🔥", 30, "a醫\n醫\nbb\n你好\nwor\nld-\ni-\n😀\n🗺\n🔥"],
  ["HelloたWorld", 50, "Hello\nた\nWorld"],
  ["HelloたWorld", 60, "Helloた\nWorld"],
  ["こんにちは〃世界", 50, "こんにちは\n〃世界"],
  ["こんにちは〃世界", 60, "こんにちは〃\n世界"],
  ["Hello た。", 70, "Hello\nた。"],
  ["Hello「たWorld」", 60, "Hello\n「た\nWorld」"],
  ["「Helloた」World", 70, "「Hello\nた」World"],
  ["  \t   Hello world", 120, "  \t   Hello\nworld"],
  ["  \t   Hello world", 60, "\nHello\nworld"],
  ["  \t   Hello world", 30, "\nHel\nlo\nwor\nld"],
  ["Hello whats up     ", 190, "Hello whats up     "],
  ["Hippopotomonstrosesquippedaliophobia        ??????", 400, "Hippopotomonstrosesquippedaliophobia\n??????"],
  ["Hippopotomonstrosesquippedaliophobia        ??????", 300, "Hippopotomonstrosesquippedalio\nphobia        ??????"],
  ["Hippopotomonstrosesquippedaliophobia        ??????", 180, "Hippopotomonstrose\nsquippedaliophobia\n??????"],
  ["Hello whats up", 70, "Hello\nwhats\nup"],
  ["Hello whats up", 15, "H\ne\nl\nl\no\nw\nh\na\nt\ns\nu\np"],
  ["Hello whats up", 130, "Hello whats\nup"],
  ["Hello whats up", 240, "Hello whats up"],
  ["Hello whats up", 50, "Hello\nwhats\nup"],
  ["Hello\n  whats up", 70, "Hello\n  whats\nup"],
  ["Hello\n  whats up", 15, "H\ne\nl\nl\no\n\nw\nh\na\nt\ns\nu\np"],
  ["Hello\n  whats up", 140, "Hello\n  whats up"],
  ["hellolongtextthisiswhatsupwithyouIamtypingggggandtypinggg break it now", 160,
    "hellolongtextthi\nsiswhatsupwithyo\nuIamtypingggggan\ndtypinggg break\nit now"],
  ["hellolongtextthisiswhatsupwithyouIamtypingggggandtypinggg break it now", 120,
    "hellolongtex\ntthisiswhats\nupwithyouIam\ntypingggggan\ndtypinggg\nbreak it now"],
  ["hellolongtextthisiswhatsupwithyouIamtypingggggandtypinggg break it now", 590,
    "hellolongtextthisiswhatsupwithyouIamtypingggggandtypinggg\nbreak it now"],
];
// textWrapping.test.ts:254-312, the CJK sentences.
const CJK_ZH = `中国你好！这是一个测试。
我们来看看：人民币¥1234「很贵」
（括号）、逗号，句号。空格 换行　全角符号…—`;
const CJK_JA = `日本こんにちは！これはテストです。
  見てみましょう：円￥1234「高い」
  （括弧）、読点、句点。
  空白 改行　全角記号…ー`;
const CJK_KO = `한국 안녕하세요! 이것은 테스트입니다.
우리 보자: 원화₩1234「비싸다」
(괄호), 쉼표, 마침표.
공백 줄바꿈　전각기호…—`;
const ORACLE_CJK = [
  [CJK_ZH, 80, `中国你好！这是一\n个测试。
我们来看看：人民\n币¥1234「很\n贵」
（括号）、逗号，\n句号。空格 换行\n全角符号…—`],
  [CJK_ZH, 50, `中国你好！\n这是一个测\n试。
我们来看\n看：人民币\n¥1234\n「很贵」
（括号）、\n逗号，句\n号。空格\n换行　全角\n符号…—`],
  [CJK_JA, 80, `日本こんにちは！\nこれはテストで\nす。
  見てみましょ\nう：円￥1234\n「高い」
  （括弧）、読\n点、句点。
  空白 改行\n全角記号…ー`],
  [CJK_JA, 50, `日本こんに\nちは！これ\nはテストで\nす。
  見てみ\nましょう：\n円\n￥1234\n「高い」
  （括\n弧）、読\n点、句点。
  空白\n改行　全角\n記号…ー`],
  [CJK_KO, 80, `한국 안녕하세\n요! 이것은 테\n스트입니다.
우리 보자: 원\n화₩1234「비\n싸다」
(괄호), 쉼\n표, 마침표.
공백 줄바꿈　전\n각기호…—`],
  [CJK_KO, 60, `한국 안녕하\n세요! 이것\n은 테스트입\n니다.
우리 보자:\n원화\n₩1234\n「비싸다」
(괄호),\n쉼표, 마침\n표.
공백 줄바꿈\n전각기호…—`],
];
const ORACLE_OFFSETS = [
  // textWrapping.test.ts:109-170
  ["Hello World!", 60, [[0, 5], [6, 12]]],
  ["  Hello  World", 90, [[0, 7], [9, 14]]],
  ["Excalidraw", 50, [[0, 5], [5, 10]]],
  ["A\n\nB", 100, [[0, 1], [2, 2], [3, 4]]],
];
const ORACLE_TOKENS = [
  // textWrapping.test.ts:461-515
  ["Excalidraw is a virtual collaborative whiteboard",
    ["Excalidraw", " ", "is", " ", "a", " ", "virtual", " ", "collaborative", " ", "whiteboard"]],
  ["Wikipedia is hosted by Wikimedia- Foundation, a non-profit organization that also hosts a range-of other projects",
    ["Wikipedia", " ", "is", " ", "hosted", " ", "by", " ", "Wikimedia-", " ", "Foundation,", " ", "a", " ", "non-", "profit", " ", "organization", " ", "that", " ", "also", " ", "hosts", " ", "a", " ", "range-", "of", " ", "other", " ", "projects"]],
  // :517-521
  ["99,100.99", ["99,100.99"]],
  // :523-583 (emoji sequences must survive byte for byte, so they are kept verbatim)
  [`😬🌍🗺🔥☂️👩🏽‍🦰👨‍👩‍👧‍👦👩🏾‍🔬🏳️‍🌈🧔‍♀️🧑‍🤝‍🧑🙅🏽‍♂️✅0️⃣🇨🇿🦅`,
    ["😬", "🌍", "🗺", "🔥", "☂️", "👩🏽‍🦰", "👨‍👩‍👧‍👦", "👩🏾‍🔬", "🏳️‍🌈", "🧔‍♀️", "🧑‍🤝‍🧑", "🙅🏽‍♂️", "✅", "0️⃣", "🇨🇿", "🦅"]],
  [`😬a🌍b🗺c🔥d☂️《👩🏽‍🦰》👨‍👩‍👧‍👦德👩🏾‍🔬こ🏳️‍🌈안🧔‍♀️g🧑‍🤝‍🧑h🙅🏽‍♂️e✅f0️⃣g🇨🇿10🦅#hash`,
    ["😬", "a", "🌍", "b", "🗺", "c", "🔥", "d", "☂️", "《", "👩🏽‍🦰", "》", "👨‍👩‍👧‍👦", "德", "👩🏾‍🔬", "こ", "🏳️‍🌈", "안", "🧔‍♀️", "g", "🧑‍🤝‍🧑", "h", "🙅🏽‍♂️", "e", "✅", "f0️⃣g", "🇨🇿", "10", "🦅", "#hash"]],
  // :585-594, every input character decomposed (NFD)
  ["c\u{30C}\u{3066}\u{3099}a\u{308}\u{3072}\u{309A}\u{3B5}\u{301}\u{1103}\u{1161}\u{438}\u{306}\u{1112}\u{1161}\u{11AB}", ["\u{10D}", "\u{3067}", "\u{E4}", "\u{3074}", "\u{3AD}", "\u{B2E4}", "\u{439}", "\u{D55C}"]],
];
// :596-701, checked with toContain only
const ORACLE_ARTIFICIAL_CJK = `《道德經》醫-醫こんにちは世界！안녕하세요세계；요』,다.다...원/달(((다)))[[1]]〚({((한))>)〛(「た」)た…[Hello] \t　World？ニューヨーク・￥3700.55す。090-1234-5678￥1,000〜＄5,000「素晴らしい！」〔重要〕＃１：Taro君30％は、（たなばた）〰￥110±￥570で20℃〜9:30〜10:00【一番】`;
const ORACLE_ARTIFICIAL_CJK_CONTAINS = ["[[1]]", "[Hello]", "World？", "Taro", "《道", "德", "經》", "醫-", "醫", "こ", "ん", "に", "ち", "は", "世", "ク・", "界！", "た…", "す。", "ュ", "「素", "晴", "ら", "し", "い！」", "君", "は、", "（た", "な", "ば", "た）", "で", "【一", "番】", "안", "녕", "하", "세", "요", "세", "계；", "요』,", "다.", "다...", "원/", "달", "(((다)))", "〚({((한))>)〛", "(「た」)", "￥3700.55", "090-", "1234-", "5678", "￥1,000〜", "＄5,000", "１：", "30％", "￥110±", "20℃〜", "9:30〜", "10:00", " ", "\t", "　", "ニ", "ー", "ヨ", "〰", "＃"];

let selfFail = 0;
const check = (what, got, expected) => {
  if (JSON.stringify(got) === JSON.stringify(expected)) return;
  selfFail++;
  console.error("SELF-CHECK FAIL", what, JSON.stringify({ got, expected }));
};
for (const [text, maxWidth, expected] of [...ORACLE_UNIT, ...ORACLE_CJK]) {
  check(`wrapText@${maxWidth}`, wrap("jsdom", text, maxWidth), expected);
}
// textWrapping.test.ts:22-27: an invalid width returns the text as it is.
for (const maxWidth of [NaN, -1, Infinity]) {
  check(`wrapText@${maxWidth}`, wrap("jsdom", "Hello Excalidraw", maxWidth), "Hello Excalidraw");
}
for (const [text, maxWidth, expected] of ORACLE_OFFSETS) {
  check(`getWrappedTextLines@${maxWidth}`, offsets("jsdom", text, maxWidth), expected);
}
for (const [text, expected] of ORACLE_TOKENS) check("parseTokens", W.parseTokens(text), expected);
const artificial = W.parseTokens(ORACLE_ARTIFICIAL_CJK);
for (const token of ORACLE_ARTIFICIAL_CJK_CONTAINS) {
  if (!artificial.includes(token)) check("parseTokens contains", token, "(missing)");
}
const selfTotal =
  ORACLE_UNIT.length + ORACLE_CJK.length + 3 + ORACLE_OFFSETS.length + ORACLE_TOKENS.length + 1;
if (selfFail) fail(`self-check: ${selfFail} oracle expectations not reproduced`, 2);
console.error(`[text-oracle] self-check: ${selfTotal}/${selfTotal} oracle expectations reproduced`);

// ---------------------------------------------------------------- corpus
let state = SEED >>> 0;
const rnd = () => {
  // mulberry32
  state = (state + 0x6d2b79f5) >>> 0;
  let t = state;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (a) => a[Math.floor(rnd() * a.length)];
const int = (lo, hi) => lo + Math.floor(rnd() * (hi - lo + 1));

const POOLS = {
  latin: ["a", "I", "to", "the", "Hello", "world", "Excalidraw", "whiteboard", "quick", "brown", "fox",
    "WAVE", "Tomorrow", "AVATAR", "Water", "offer", "r.", "mmm", "illicit", "café", "naïve", "Ærøskøbing"],
  long: ["Hippopotomonstrosesquippedaliophobia", "supercalifragilisticexpialidocious",
    "https://excalidraw.com/#json=abc123,def456", "a_very_long_identifier_name_for_testing",
    "WWWWWWWWWWWWWWWW", "iiiiiiiiiiiiiiiiiiiiiiiiiiii", "0123456789012345678901234567890"],
  hyphen: ["state-of-the-art", "non-profit", "---", "-lead", "trail-", "a-b-c-d-e", "x--y", "e-mail"],
  punct: ["(hello)", "[x]", "{y}", "a,b", "end.", "wait...", "a/b/c", "x:y;z", "why?", "wow!", "…",
    "<tag>", "(((nested)))", "f(x)(y)", "99,100.99", "#hash", "a.b.c.d", "!?", "\"quoted\"", "'q'"],
  cjk: ["こんにちは世界", "你好，世界。", "「引用」", "안녕하세요", "テスト・ケース", "￥1000", "（括弧）",
    "ｺﾝﾆﾁﾊ", "〃", "ー", "中国你好！", "【一番】", "は、", "Taro君", "日本語テキスト", "한국어 문장"],
  emoji: ["😀", "🗺", "👩🏽‍🦰", "👨‍👩‍👧‍👦", "🇨🇿", "☂️", "0️⃣", "✅", "🏳️‍🌈", "🧑‍🤝‍🧑", "🏴󠁧󠁢󠁳󠁣󠁴󠁿", "🔥🔥🔥", "👍🏽"],
  ws: [" ", "  ", "     ", "\t", "\t\t", "\u00a0", "\u3000", "\u2003", "\ufeff", "\u0085", "\u200b", "\u202f", "\r", "\u2028"],
  combining: ["e\u0301", "c\u030c", "\u3066\u3099", "\u1100\u1161", "a\u0308", "o\u0302\u0301", "\u00e9"],
  astral: ["𝐀𝐁𝐂", "𠀀𠀁", "𝔘𝔫𝔦", "𝟘𝟙𝟚"],
  code: ["function foo(bar) {", "  return bar.map((x) => x * 2);", "}", "const url = 'https://example.com/a/b?c=d&e=f';",
    "if (a && b || !c) { x += 1; }", "\tindented();", "// comment with many     spaces", "SELECT * FROM t WHERE a='b';",
    "<div class=\"x\">hi</div>", "fn main() -> Result<(), Box<dyn Error>> {", "arr[i][j] = {k: v};",
    "    deeply.nested.call(arg1, arg2, arg3);", "x=>y", "a+b*c-d/e", "foo_bar_baz(qux)", "#include <stdio.h>"],
};
const CATS = Object.keys(POOLS);
const SEPS = [" ", " ", " ", "", "  ", "\n", "   ", "\t", "\n\n"];

const makeText = () => {
  const n = int(1, 14);
  const focus = pick(CATS);
  const used = new Set();
  let out = "";
  if (rnd() < 0.08) out += pick(["  ", "\t", "\n", "   "]); // leading whitespace or newline
  for (let i = 0; i < n; i++) {
    const cat = rnd() < 0.55 ? focus : pick(CATS);
    used.add(cat);
    out += pick(POOLS[cat]);
    if (i < n - 1) out += focus === "code" ? pick(["\n", " ", "\n  ", "\n\t"]) : pick(SEPS);
  }
  if (rnd() < 0.15) out += pick(["  ", "     ", "\t", "\n", " "]); // trailing
  return { text: out, tags: [...used].sort() };
};

const pickWidth = (model, text) => {
  const m = MODELS[model];
  const lines = text.split("\n");
  const maxLine = Math.max(...lines.map(m));
  const chars = Array.from(pick(lines));
  const prefix = m(chars.slice(0, int(0, chars.length)).join(""));
  const step = model === "jsdom" ? 10 : 0.25;
  const r = rnd();
  if (r < 0.3) return prefix; // exactly on a boundary: exercises `<=`
  if (r < 0.4) return prefix + step;
  if (r < 0.5) return Math.max(0, prefix - step);
  if (r < 0.85) return Math.round(rnd() * Math.max(maxLine, 40) * 4) / 4;
  return pick([0, 1, 5, 8, 9.5, 1e6, -1]);
};

const cases = [];
const addCase = (id, model, tags, text, maxWidth, expected) => {
  cases.push({
    id,
    model,
    tags,
    text,
    maxWidth,
    expected: expected ?? wrap(model, text, maxWidth),
    // Offsets are code units of the NFC text (textWrapping.ts:378-381 says so): only a
    // text NFC leaves alone can compare them against the source.
    offsets: text === text.normalize("NFC") ? offsets(model, text, maxWidth) : null,
  });
};
[...ORACLE_UNIT, ...ORACLE_CJK].forEach(([text, maxWidth, expected], i) =>
  addCase(`unit-${i}`, "jsdom", ["oracle-unit"], text, maxWidth, expected),
);
for (let i = 0; i < COUNT; i++) {
  const { text, tags } = makeText();
  const model = pick(["table", "table", "kern", "jsdom"]);
  addCase(`fuzz-${i}`, model, tags, text, pickWidth(model, text));
}

// ---------------------------------------------------------------- tokenizer fixtures
// Every distinct hard line of the corpus, the oracle's own parseTokens inputs, and probes
// that pin each character class the break rules name. The probe characters are the
// punctuation, symbol and fullwidth blocks around the oracle's literal sets
// (textWrapping.ts:61-118), each in contexts that fire every rule on either side of it.
const PROBE_BLOCKS = [
  [0x21, 0x2f], [0x3a, 0x40], [0x5b, 0x60], [0x7b, 0x7e], [0xa0, 0xbf], [0x2000, 0x206f],
  [0x20a0, 0x20c0], [0x2100, 0x215f], [0x3000, 0x303f], [0x3099, 0x309f], [0x30fb, 0x30ff],
  [0xfe30, 0xfe4f], [0xff00, 0xffef],
];
const probeLines = [];
for (const [lo, hi] of PROBE_BLOCKS) {
  for (let cp = lo; cp <= hi; cp++) {
    const c = String.fromCodePoint(cp);
    probeLines.push(`a${c}b`, `中${c}国`, `${c}${c}a`, `(${c}`, `${c})`, `-${c}「`);
  }
}
const seen = new Set();
const tokens = [];
const addTokens = (line) => {
  if (!line || seen.has(line)) return;
  seen.add(line);
  tokens.push({ line, tokens: W.parseTokens(line) });
};
for (const [text] of ORACLE_TOKENS) addTokens(text);
addTokens(ORACLE_ARTIFICIAL_CJK);
for (const c of cases) c.text.split("\n").forEach(addTokens);
probeLines.forEach(addTokens);

// ---------------------------------------------------------------- Unicode tables
const ranges = (member) => {
  const out = [];
  let start = -1;
  for (let cp = 0; cp <= 0x110000; cp++) {
    const inSet = cp < 0x110000 && (cp < 0xd800 || cp > 0xdfff) && member(cp);
    if (inSet && start < 0) start = cp;
    if (!inSet && start >= 0) {
      out.push([start, cp - 1]);
      start = -1;
    }
  }
  return out;
};
const byRegex = (re) => ranges((cp) => re.test(String.fromCodePoint(cp)));

// Two NFC properties the port relies on, neither of which JavaScript exposes, so both are
// derived from `normalize` itself:
//
// NFC_UNSTABLE — not (canonical combining class 0 and NFC_QC=Yes). A line with none of
//   these is its own NFC, so the port skips the normaliser for it.
// NFC_CONTINUES — no normalisation boundary before the char: its decomposition starts with
//   a non-starter or with a char that may compose with what precedes it. The port
//   normalises the segments between boundaries one by one, so each piece of the NFC text
//   maps back to the piece of source it came from.
//
// A non-starter reorders against U+0345 (ccc 240) or U+0334 (ccc 1); a char that may
// combine backward (NFC_QC=Maybe) appears after the first position of some canonical
// decomposition; a QC=No char changes on its own.
const nonStarter = (s) =>
  ("\u0345" + s).normalize("NFD") !== "\u0345" + s || (s + "\u0334").normalize("NFD") !== s + "\u0334";
const unstable = new Uint8Array(0x110000);
const joinsBackward = new Uint8Array(0x110000);
const firstOfNfd = new Uint32Array(0x110000);
for (let cp = 0; cp < 0x110000; cp++) {
  if (cp >= 0xd800 && cp <= 0xdfff) continue;
  const s = String.fromCodePoint(cp);
  const nfd = Array.from(s.normalize("NFD"));
  firstOfNfd[cp] = nfd[0].codePointAt(0);
  if (s.normalize("NFC") !== s || (nfd.length === 1 && nonStarter(s))) unstable[cp] = 1;
  for (const c of nfd.slice(1)) joinsBackward[c.codePointAt(0)] = 1;
}
const continues = new Uint8Array(0x110000);
for (let cp = 0; cp < 0x110000; cp++) {
  if (cp >= 0xd800 && cp <= 0xdfff) continue;
  const first = firstOfNfd[cp];
  if (joinsBackward[first] || nonStarter(String.fromCodePoint(first))) continues[cp] = unstable[cp] = 1;
}
// ...and check what the port relies on: NFC of a line is the NFC of its segments, over
// every line generated here and every char with a boundary that NFC still rewrites.
const segmentsNfc = (line) => {
  let out = "";
  let seg = "";
  for (const c of line) {
    if (seg && !continues[c.codePointAt(0)]) {
      out += seg.normalize("NFC");
      seg = "";
    }
    seg += c;
  }
  return out + seg.normalize("NFC");
};
const nfcChecks = tokens.map(({ line }) => line);
for (let cp = 0; cp < 0x110000; cp++) {
  if (unstable[cp] && !continues[cp]) {
    const c = String.fromCodePoint(cp);
    nfcChecks.push(`a${c}`, `e\u0301${c}\u0301`, `\u1100${c}\u1161`, `\u0b47${c}\u0b3e`, ` ${c}\u0327\u0301`);
  }
}
for (const line of nfcChecks) {
  if (segmentsNfc(line) !== line.normalize("NFC")) fail(`NFC segments disagree on ${JSON.stringify(line)}`, 2);
}

const TABLES = [
  ["WHITESPACE", "`\\s` — WhiteSpace and LineTerminator (COMMON.WHITESPACE, textWrapping.ts:70).", byRegex(/^\s$/u)],
  [
    "CJK_SCRIPT",
    "Han, Hiragana, Katakana and Hangul (the script half of CJK.CHAR, textWrapping.ts:95).",
    byRegex(/^[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]$/u),
  ],
  ["EMOJI", "`\\p{Emoji}` (EMOJI.ANY, textWrapping.ts:125).", byRegex(/^\p{Emoji}$/u)],
  [
    "EMOJI_START",
    "`[\\p{Extended_Pictographic}\\p{Emoji_Presentation}]` (EMOJI.MOST, textWrapping.ts:126).",
    byRegex(/^[\p{Extended_Pictographic}\p{Emoji_Presentation}]$/u),
  ],
  ["EMOJI_MODIFIER", "`\\p{Emoji_Modifier}` (EMOJI.JOINER, textWrapping.ts:122-123).", byRegex(/^\p{Emoji_Modifier}$/u)],
  ["REGIONAL_INDICATOR", "`\\p{RI}` (EMOJI.FLAG, textWrapping.ts:121).", byRegex(/^\p{RI}$/u)],
  [
    "NFC_UNSTABLE",
    "Not (ccc = 0 and NFC_QC = Yes): NFC may change it or its neighbours.",
    ranges((cp) => unstable[cp] === 1),
  ],
  [
    "NFC_CONTINUES",
    "No NFC boundary before it: it may reorder or compose with what precedes it.",
    ranges((cp) => continues[cp] === 1),
  ],
];

const hex = (n) => `0x${n.toString(16).toUpperCase().padStart(4, "0")}`;
const rust = [
  "//! GENERATED by `engine/tools/text-oracle/generate.mjs` — do not edit; regenerate with",
  "//! the root's `make oracle-fixtures`.",
  "//!",
  `//! Evaluated by the regex engine of Node ${process.version} (Unicode ${process.versions.unicode}),`,
  "//! the engine the oracle's own regexes run on. Sorted, disjoint, inclusive code-point",
  "//! ranges, searched by `text::wrap`.",
  "",
];
for (const [name, doc, table] of TABLES) {
  rust.push(`/// ${doc}`, "#[rustfmt::skip]", `pub(crate) const ${name}: &[(u32, u32)] = &[`);
  for (let i = 0; i < table.length; i += 4) {
    rust.push(`    ${table.slice(i, i + 4).map(([a, b]) => `(${hex(a)}, ${hex(b)}),`).join(" ")}`);
  }
  rust.push("];", "");
}
rust.pop();

// ---------------------------------------------------------------- write
if (!existsSync(dirname(FIXTURE)) || !existsSync(dirname(UNICODE_RS))) fail("run from the engine checkout");
const usedCps = new Set();
for (const c of cases) for (const cp of cps(c.text + c.expected)) usedCps.add(cp);
const widths = {};
for (const cp of [...usedCps].sort((a, b) => a - b)) widths[cp] = tableWidth(cp);

const head = {
  generator: "engine/tools/text-oracle/generate.mjs",
  oracle: { excalidraw: PIN, files: ORACLE_FILES },
  node: process.version,
  unicode: process.versions.unicode,
  seed: SEED,
  count: COUNT,
  models: {
    jsdom: "width(s) = 10 * s.length (UTF-16 code units)",
    table: { note: "width(s) = sum of widths[code point]", widths },
    kern: {
      note: "line width = table + sum of pairs[a + b] over adjacent code points; one char is unkerned",
      pairs: KERN,
    },
  },
};
// One case per line: diffs of a regenerated fixture stay readable.
const body = (key, items) =>
  `  ${JSON.stringify(key)}: [\n${items.map((x) => `    ${JSON.stringify(x)}`).join(",\n")}\n  ]`;
const json = `${JSON.stringify(head, null, 1).slice(0, -2)},\n${body("cases", cases)},\n${body("tokens", tokens)}\n}\n`;
JSON.parse(json);
writeFileSync(FIXTURE, json);
writeFileSync(UNICODE_RS, `${rust.join("\n")}\n`);

const byModel = {};
for (const c of cases) byModel[c.model] = (byModel[c.model] ?? 0) + 1;
console.error(
  `[text-oracle] ${cases.length} cases ${JSON.stringify(byModel)}, ` +
    `${cases.filter((c) => c.expected !== c.text).length} changed by wrapping, ${tokens.length} token lines, ` +
    `${json.length} bytes; tables: ${TABLES.map(([n, , t]) => `${n} ${t.length}`).join(", ")}`,
);
