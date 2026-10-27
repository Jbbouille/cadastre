// Tokenizes snippets with VS Code's TextMate engine and checks the scopes of chosen tokens.
// Usage: node test/grammar.test.js [file.ca ...]   (with files: prints their tokens)

const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const vsctm = require("vscode-textmate");
const oniguruma = require("vscode-oniguruma");

const wasm = fs.readFileSync(require.resolve("vscode-oniguruma/release/onig.wasm")).buffer;
const onigLib = oniguruma.loadWASM(wasm).then(() => ({
  createOnigScanner: (patterns) => new oniguruma.OnigScanner(patterns),
  createOnigString: (s) => new oniguruma.OnigString(s),
}));
const registry = new vsctm.Registry({
  onigLib,
  loadGrammar: async () =>
    vsctm.parseRawGrammar(
      fs.readFileSync(path.join(__dirname, "..", "syntaxes", "cadastre.tmLanguage.json"), "utf8"),
      "cadastre.tmLanguage.json",
    ),
});

/** [text, innermost scope] of every token of a text. */
function tokenize(grammar, text) {
  let state = vsctm.INITIAL;
  const tokens = [];
  for (const line of text.split(/\r?\n/)) {
    const result = grammar.tokenizeLine(line, state);
    for (const t of result.tokens) {
      const value = line.slice(t.startIndex, t.endIndex);
      if (value.trim()) tokens.push([value.trim(), t.scopes[t.scopes.length - 1]]);
    }
    state = result.ruleStack;
  }
  return tokens;
}

function expectScopes(grammar, text, expected) {
  const tokens = tokenize(grammar, text);
  for (const [value, scope] of expected) {
    const found = tokens.filter(([v]) => v === value).map(([, s]) => s);
    assert.ok(found.includes(scope), `${JSON.stringify(value)} in ${JSON.stringify(text)}: expected ${scope}, got ${found.join(", ") || "nothing"}`);
  }
}

const cases = [
  ["check owners { select app where not exists(owner) expect none severity: warning }", [
    ["check", "keyword.other.cadastre"],
    ["owners", "entity.name.function.view.cadastre"],
    ["expect", "keyword.other.cadastre"],
    ["none", "keyword.other.cadastre"],
    ["severity", "keyword.other.cadastre"],
    ["warning", "constant.language.cadastre"],
  ]],
  ["check c { select server expect all os != debian9 }", [
    ["all", "keyword.other.cadastre"],
  ]],
  ["server web01 { os: debian12, ip: 10.0.1.5 }", [
    ["server", "entity.name.type.cadastre"],
    ["web01", "variable.other.definition.cadastre"],
    ["os", "variable.other.property.cadastre"],
    ["10.0.1.5", "constant.other.address.cadastre"],
  ]],
  ["  -> crm : https/443 [flow, auth] { carries: [customer] }", [
    ["->", "keyword.operator.arrow.cadastre"],
    ["crm", "variable.other.cadastre"],
    ["https", "support.constant.protocol.cadastre"],
    ["443", "constant.numeric.port.cadastre"],
    ["flow", "entity.name.tag.layer.cadastre"],
    ["auth", "entity.name.tag.layer.cadastre"],
    ["carries", "variable.other.property.cadastre"],
  ]],
  ["billing.api -> crm.db : tcp/5432-5433", [
    ["crm.db", "variable.other.cadastre"],
    ["5432-5433", "constant.numeric.port.cadastre"],
  ]],
  ["subnet dmz { cidr: 10.0.1.0/24, v6: 2001:db8::/32, version: 1.2.3-rc.1, weight: 2.5 }", [
    ["subnet", "entity.name.type.cadastre"],
    ["10.0.1.0/24", "constant.other.address.cadastre"],
    ["2001:db8::/32", "constant.other.address.cadastre"],
    ["1.2.3-rc.1", "constant.numeric.version.cadastre"],
    ["2.5", "constant.numeric.cadastre"],
  ]],
  ['app billing { description: "Monthly \\"invoicing\\"" } // done', [
    ['"', "string.quoted.double.cadastre"],
    ["\\\"", "constant.character.escape.cadastre"],
    ["// done", "comment.line.double-slash.cadastre"],
  ]],
  ["view price-chain {", [
    ["view", "keyword.other.cadastre"],
    ["price-chain", "entity.name.function.view.cadastre"],
  ]],
  ["  select app, job, server without isolated group-by site", [
    ["select", "keyword.other.cadastre"],
    ["without", "keyword.other.cadastre"],
    ["isolated", "keyword.other.cadastre"],
    ["group-by", "keyword.other.cadastre"],
  ]],
  ["  select system where kind = system or not exists(parent)", [
    ["where", "keyword.other.cadastre"],
    ["or", "keyword.operator.word.cadastre"],
    ["not", "keyword.operator.word.cadastre"],
    ["exists", "keyword.other.cadastre"],
    ["=", "keyword.operator.cadastre"],
  ]],
  ["  edges lift([flow], via: system?.ancestor(system))", [
    ["lift", "keyword.other.cadastre"],
    ["via", "variable.other.property.cadastre"],
    ["?", "keyword.operator.optional.cadastre"],
    ["ancestor", "keyword.other.cadastre"],
  ]],
  ["  edges where proto in [http, https] { color: \"#1d4ed8\" }", [
    ["edges", "keyword.other.cadastre"],
    ["in", "keyword.operator.word.cadastre"],
    ["color", "variable.other.property.cadastre"],
  ]],
  ["/* block\n   comment */ app a", [
    ["/*", "comment.block.cadastre"],
    ["block", "comment.block.cadastre"],
    ["comment", "comment.block.cadastre"],
    ["*/", "comment.block.cadastre"],
    ["app", "entity.name.type.cadastre"],
  ]],
];

registry.loadGrammar("source.cadastre").then((grammar) => {
  const files = process.argv.slice(2);
  if (files.length) {
    for (const file of files) {
      for (const [value, scope] of tokenize(grammar, fs.readFileSync(file, "utf8"))) {
        console.log(`${scope.padEnd(45)} ${value}`);
      }
    }
    return;
  }
  // The resource pattern must not take keywords for a kind and a name.
  const tokens = tokenize(grammar, "  select app where criticality >= 2\n  render svg, mermaid { legend: true }");
  assert.ok(!tokens.some(([, s]) => s === "entity.name.type.cadastre"), JSON.stringify(tokens));
  for (const [text, expected] of cases) expectScopes(grammar, text, expected);
  console.log(`grammar: ${cases.length + 1} cases passed`);
}).catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
