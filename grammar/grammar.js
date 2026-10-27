/**
 * Tree-sitter grammar for Cadastre.
 *
 * A single grammar for registries (.ca) and plans (.cav):
 * the compiler is the one rejecting a resource in a .cav or a view in a .ca.
 * Guide: ../docs/guide.md, reference: ../docs/reference.md
 *
 * @file Cadastre grammar for tree-sitter
 * @license MIT OR Apache-2.0
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

// `$code` is a variable of a `for_each`, also inside a name: `srv-$code`.
const IDENT = /[a-zA-Z_$][a-zA-Z0-9_$]*(-[a-zA-Z0-9_$]+)*/;
const IDENT_SRC = '[a-zA-Z_$][a-zA-Z0-9_$]*(-[a-zA-Z0-9_$]+)*';

const OCTET = '\\d{1,3}';
const IPV4_SRC = `${OCTET}(\\.${OCTET}){3}`;

const H16 = '[0-9a-fA-F]{1,4}';
const IPV6_SRC =
  `((${H16}:){7}${H16}` +
  `|((${H16}:){0,6}${H16})?::((${H16}:){0,6}${H16})?)`;

const SEMVER_ID = '[0-9A-Za-z-]+(\\.[0-9A-Za-z-]+)*';

const PREC = {
  or: 1,
  and: 2,
  not: 3,
};

/**
 * Comma-separated list, at least one item.
 * @param {RuleOrLiteral} rule
 */
function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}

/**
 * Block elements, separated by whitespace or an optional comma.
 * @param {RuleOrLiteral} rule
 */
function optionalCommaSep(rule) {
  return repeat(seq(rule, optional(',')));
}

module.exports = grammar({
  name: 'cadastre',

  word: $ => $.identifier,

  extras: $ => [/\s/, $.comment],

  // `count(on) >= 2`: an aggregate of a path, or a key with an argument; the compiler tells them
  // apart by the function (`count`).
  conflicts: $ => [[$.path_step, $.call]],

  rules: {
    source_file: $ => repeat(choice(
      $.kind_declaration,
      $.link_declaration,
      $.group_declaration,
      $.process,
      $.resource,
      $.link,
      $.for_each,
      $.view,
      $.check,
      $.style_block,
    )),

    // Resources and links declared once per item of a list, its variables replaced:
    // for_each (code, ip) in [[lil, 10.1.1.10], [lyo, 10.1.2.10]] { server srv-$code { ip: $ip } }
    for_each: $ => seq(
      'for_each',
      choice(
        field('variable', $.identifier),
        seq('(', commaSep1(field('variable', $.identifier)), ')'),
      ),
      'in',
      field('items', choice($.list, $.csv)),
      '{',
      repeat(choice($.resource, $.link, $.inline_link, $.for_each)),
      '}',
    ),

    // ------------------------------------------------------------------
    // Schema of the project
    // ------------------------------------------------------------------

    // Keys of a kind, built in or of the project (`like` a built-in one):
    // kind server { os: string, ram: number, vmware_cluster: ref(cluster), inventory_id: string (required) }
    // kind printer { like: device, parent: site | subnet, serial: pattern("SN-[0-9]+") }
    kind_declaration: $ => seq(
      'kind',
      field('name', $.identifier),
      '{',
      optionalCommaSep(choice($.key_declaration, $.comment)),
      '}',
    ),

    // A group of kinds of the project, usable wherever a kind is: `group storage: database | saas`.
    group_declaration: $ => seq(
      'group',
      field('name', $.identifier),
      ':',
      field('member', $.identifier),
      repeat(seq('|', field('member', $.identifier))),
    ),

    // The rows of a CSV file, its path relative to the file writing it: `csv("stores.csv")`.
    csv: $ => seq('csv', '(', field('path', $.string), ')'),

    // Keys of the links: all of them, or those of a layer.
    // link { volume: string }   layer flow { package: string }
    link_declaration: $ => seq(
      choice('link', seq('layer', field('layer', $.identifier))),
      '{',
      optionalCommaSep(choice($.key_declaration, $.comment)),
      '}',
    ),

    key_declaration: $ => seq(
      field('key', $.identifier),
      ':',
      field('type', $.type),
      optional(field('required', $.required)),
    ),

    required: _ => seq('(', 'required', ')'),

    // A type: a named one (`string`, `number`, `ip`…), a reference, a pattern, or a closed list of
    // identifiers; `+` one or more, `*` zero or more, `{n}` exactly n.
    type: $ => choice(
      seq(
        choice(
          field('name', $.identifier),
          seq('ref', token.immediate('('), field('kinds', $.kinds), ')'),
          seq('pattern', token.immediate('('), field('pattern', $.string), ')'),
        ),
        optional(field('cardinality', $.cardinality)),
      ),
      field('values', $.choices),
    ),

    // Kinds or groups of a reference: `cluster`, `server | cluster`.
    kinds: $ => seq($.identifier, repeat(seq('|', $.identifier))),

    // A closed list of values, or of kinds for `parent`: `vmware | hyperv`.
    choices: $ => seq($.identifier, repeat1(seq('|', $.identifier))),

    cardinality: $ => choice(
      token.immediate('+'),
      token.immediate('*'),
      seq(token.immediate('{'), $.number, '}'),
    ),

    // ------------------------------------------------------------------
    // Registry
    // ------------------------------------------------------------------

    // `virtual server web01 { on: esx01 }`: a machine running on a host.
    resource: $ => seq(
      optional(field('virtual', 'virtual')),
      field('kind', $.identifier),
      field('name', $.identifier),
      optional(field('body', $.block)),
    ),

    block: $ => seq(
      '{',
      optionalCommaSep(choice(
        $.attribute,
        $.resource,
        $.process,
        $.inline_link,
        $.for_each,
        // The members of a pool, chosen by a filter: `pool stores { select server where … }`.
        $.selection,
      )),
      '}',
    ),

    attribute: $ => seq(
      field('key', $.identifier),
      ':',
      field('value', $._value),
    ),

    // A business process: its steps in their order, each a link already declared (or the reply
    // to one), or a processing within a resource; loops of steps.
    // process orders { step shop -> erp : https { async }  step erp "computes the prices" }
    process: $ => seq(
      'process',
      field('name', $.identifier),
      '{',
      optionalCommaSep(choice($.attribute, $.participant, $.step, $.loop, $.comment)),
      '}',
    ),

    // A participant of the process standing for several resources, named in its steps: one
    // participant of its sequence diagram, a link checked for each member.
    // participant esx "ESXi hosts" [esx1, esx2]
    participant: $ => seq(
      'participant',
      field('name', $.identifier),
      optional(field('label', $.string)),
      field('members', $.list),
    ),

    step: $ => seq(
      'step',
      field('source', $._reference),
      choice(
        seq(
          '->',
          field('target', $._reference),
          optional(seq(':', field('port', $.port_spec))),
          optional(field('layers', $.layers)),
        ),
        field('text', $.string),
      ),
      optional(field('body', $.step_block)),
    ),

    // Attributes of a step (`label`, `note`) and its flags (`async`, `reply`).
    step_block: $ => seq(
      '{',
      optionalCommaSep(choice($.attribute, field('flag', $.identifier))),
      '}',
    ),

    loop: $ => seq(
      'loop',
      field('label', $.string),
      '{',
      optionalCommaSep(choice($.step, $.loop, $.comment)),
      '}',
    ),

    // Full link, at the root level.
    link: $ => seq(
      field('source', $._reference),
      '->',
      $._link_tail,
    ),

    // Link with an implicit source, inside a resource block.
    inline_link: $ => seq(
      '->',
      $._link_tail,
    ),

    _link_tail: $ => seq(
      field('target', $._reference),
      optional(seq(':', field('port', $.port_spec))),
      optional(field('layers', $.layers)),
      optional(field('body', $.link_block)),
    ),

    link_block: $ => seq(
      '{',
      optionalCommaSep($.attribute),
      '}',
    ),

    port_spec: $ => seq(
      field('protocol', $.identifier),
      optional(seq(
        token.immediate('/'),
        field('port', alias(token.immediate(/\d+(-\d+)?/), $.port)),
      )),
    ),

    layers: $ => seq(
      '[',
      optional(seq(commaSep1($.identifier), optional(','))),
      ']',
    ),

    // ------------------------------------------------------------------
    // Values
    // ------------------------------------------------------------------

    _reference: $ => choice($.identifier, $.qualified_name),

    _value: $ => choice(
      $._scalar,
      $.list,
    ),

    _scalar: $ => choice(
      $.identifier,
      $.qualified_name,
      $.number,
      $.quantity,
      $.semver,
      $.ipv4,
      $.cidr4,
      $.ipv6,
      $.cidr6,
      $.string,
    ),

    // A list of scalars; nested lists group items (the levels of `stack: [[a, b], c]`), and the
    // compiler only accepts them there.
    list: $ => seq(
      '[',
      optional(seq(commaSep1(choice($._scalar, $.list)), optional(','))),
      ']',
    ),

    identifier: _ => IDENT,

    qualified_name: _ => token(new RegExp(`${IDENT_SRC}(\\.${IDENT_SRC})+`)),

    number: _ => token(/\d+(\.\d+)?/),

    // A number and its unit, without a space: `100Mbps`, `1.5Gbit/s`.
    quantity: _ => token(/\d+(\.\d+)?[a-zA-Z][a-zA-Z]*(\/[a-zA-Z]+)?/),

    semver: _ => token(new RegExp(
      `v?\\d+\\.\\d+\\.\\d+(-${SEMVER_ID})?(\\+${SEMVER_ID})?`,
    )),

    ipv4: _ => token(new RegExp(IPV4_SRC)),

    cidr4: _ => token(new RegExp(`${IPV4_SRC}/\\d{1,2}`)),

    ipv6: _ => token(new RegExp(IPV6_SRC)),

    cidr6: _ => token(new RegExp(`${IPV6_SRC}/\\d{1,3}`)),

    string: _ => token(seq(
      '"',
      repeat(choice(/[^"\\\n]+/, /\\./)),
      '"',
    )),

    comment: _ => token(choice(
      seq('//', /[^\r\n]*/),
      seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'),
    )),

    // ------------------------------------------------------------------
    // Plans
    // ------------------------------------------------------------------

    view: $ => seq(
      'view',
      field('name', $.identifier),
      '{',
      repeat(choice(
        $.select_clause,
        $.zone_clause,
        $.edges_clause,
        $.style_block,
        $.render_clause,
        // `title: "…"`, `description: "…"`: what the view is, for its readers.
        $.attribute,
      )),
      '}',
    ),

    // A rule on the model, run by `cadastre check`: a selection, as in a view, and its
    // expected result.
    //   check every-app-has-an-owner { select app where not exists(owner) expect none }
    check: $ => seq(
      'check',
      field('name', $.identifier),
      '{',
      repeat(choice(
        $.select_clause,
        $.edges_clause,
        $.expect_clause,
        $.attribute,
      )),
      '}',
    ),

    expect_clause: $ => seq(
      'expect',
      field('expectation', choice(
        alias('none', $.none),
        alias('some', $.some),
        $.count,
        $.all,
      )),
    ),

    count: $ => seq(
      'count',
      field('operator', choice('=', '!=', '<', '<=', '>', '>=')),
      field('value', $.number),
    ),

    all: $ => seq(
      'all',
      $._predicate,
    ),

    // A named frame of the view, gathering its members whatever their group-by frames:
    // zone hostco-sap "Hostco SAP" [hostco-site-east, hostco-site-south] { fill: "#eef" }
    zone_clause: $ => seq(
      'zone',
      field('name', $.identifier),
      optional(field('label', $.string)),
      choice(field('members', $.list), $.where_clause),
      optional($._style_properties),
    ),

    select_clause: $ => seq(
      'select',
      commaSep1(field('kind', $.identifier)),
      optional($.where_clause),
      optional($.with_ends),
      optional($.without_isolated),
      optional($.group_by),
    ),

    // A selection in the registry: the kinds, then a filter; a comma after a kind continues the
    // list rather than separating the items of the block.
    selection: $ => prec.right(seq(
      // `select` then a space: `select: 1` stays a key of the block.
      alias(token(seq('select', /\s+/)), 'select'),
      commaSep1(field('kind', $.identifier)),
      optional($.where_clause),
    )),

    // Adds the ends of the selected edges to the nodes.
    with_ends: _ => seq('with', 'ends'),

    // Removes the nodes without any edge.
    without_isolated: _ => seq('without', 'isolated'),

    group_by: $ => seq(
      'group-by',
      field('level', $.identifier),
      repeat(seq('>', field('level', $.identifier))),
    ),

    edges_clause: $ => seq(
      'edges',
      commaSep1(choice(
        $.edge_selector,
        $.edges_option,
      )),
    ),

    edge_selector: $ => seq(
      field('selector', $._selector),
      optional($.where_clause),
    ),

    _selector: $ => choice(
      $.layers,
      $.identifier,
      $.lift,
      $.follow,
      $.process_selector,
      $.path_selector,
    ),

    // The links of a business process: `process(orders)`.
    process_selector: $ => seq(
      'process',
      token.immediate('('),
      field('name', $._reference),
      ')',
    ),

    // The network path between two resources, deduced from the cabling: `path(proxy, esx-02)`.
    path_selector: $ => seq(
      'path',
      token.immediate('('),
      field('from', $._reference),
      ',',
      field('to', $._reference),
      ')',
    ),

    edges_option: $ => seq(
      field('key', $.identifier),
      ':',
      field('value', $.identifier),
    ),

    lift: $ => seq(
      'lift',
      '(',
      field('edges', $.edge_selector),
      repeat(seq(',', $.lift_option)),
      ')',
    ),

    lift_option: $ => seq(
      field('key', $.identifier),
      ':',
      field('value', $.path),
    ),

    // follow(item-prices, from: pricing.ui): the links carrying a data object,
    // walked in the direction of the data.
    follow: $ => seq(
      'follow',
      '(',
      field('data', $._reference),
      repeat(seq(',', $.follow_option)),
      ')',
    ),

    follow_option: $ => seq(
      field('key', $.identifier),
      ':',
      field('value', choice($._reference, $.list)),
    ),

    // on?.ancestor(site): steps joined by a dot with no whitespace.
    path: $ => seq(
      $.path_step,
      repeat(seq(token.immediate('.'), $.path_step)),
    ),

    path_step: $ => seq(
      field('step', choice(
        $.identifier,
        $.ancestor,
        $.call,
      )),
      optional(field('optional', alias(token.immediate('?'), $.optional_marker))),
    ),

    ancestor: $ => seq(
      'ancestor',
      '(',
      field('kind', $.identifier),
      ')',
    ),

    // A key with an argument: `failures(hosting)`.
    call: $ => seq(
      field('function', $.identifier),
      token.immediate('('),
      field('argument', $.identifier),
      ')',
    ),

    where_clause: $ => seq(
      'where',
      $._predicate,
    ),

    _predicate: $ => choice(
      $.flag,
      $.comparison,
      $.has,
      $.in,
      $.exists,
      $.and,
      $.or,
      $.not,
      $.parenthesized,
    ),

    // Boolean attribute alone: `where incomplete`.
    flag: $ => field('attribute', $.path),

    // An aggregate of the items a path gives: `count(on.ancestor(site)) >= 2`. The function is a
    // name, so that `count` stays usable as a key (`where count = 2`); the compiler knows `count`.
    aggregate: $ => prec.dynamic(1, seq(
      field('function', $.identifier),
      token.immediate('('),
      field('path', $.path),
      ')',
    )),

    comparison: $ => seq(
      field('left', choice($.path, $.aggregate)),
      field('operator', choice('=', '!=', '<', '<=', '>', '>=')),
      // A value, or a path of an end of an edge (`source.ancestor(system) != target.ancestor(system)`):
      // the compiler reads a right side starting with `source` or `target` as a path.
      field('right', choice($._scalar, $.right_path)),
    ),

    // A path on the right of a comparison, beyond what a name already covers (`target.owner` is a
    // qualified name): its first token is a name, the lexer reading `target.ancestor` as one, then
    // the kind of an `ancestor`, a `?`, or more steps.
    right_path: $ => seq(
      field('head', choice($.identifier, $.qualified_name)),
      choice(
        seq(
          token.immediate('('),
          field('kind', $.identifier),
          ')',
          optional(field('optional', alias(token.immediate('?'), $.optional_marker))),
        ),
        field('optional', alias(token.immediate('?'), $.optional_marker)),
      ),
      repeat(seq(token.immediate('.'), $.path_step)),
    ),

    has: $ => seq(
      field('left', $.path),
      'has',
      field('right', $._scalar),
    ),

    in: $ => seq(
      field('left', $.path),
      'in',
      field('right', $.list),
    ),

    exists: $ => seq(
      'exists',
      '(',
      field('attribute', $.path),
      ')',
    ),

    and: $ => prec.left(PREC.and, seq($._predicate, 'and', $._predicate)),

    or: $ => prec.left(PREC.or, seq($._predicate, 'or', $._predicate)),

    not: $ => prec(PREC.not, seq('not', $._predicate)),

    parenthesized: $ => seq('(', $._predicate, ')'),

    style_block: $ => seq(
      'style',
      '{',
      repeat(choice($.layer_style, $.kind_style, $.nodes_style, $.edges_style)),
      '}',
    ),

    layer_style: $ => seq(
      'layer',
      field('layer', $.identifier),
      optional($.where_clause),
      $._style_properties,
    ),

    kind_style: $ => seq(
      'kind',
      field('kind', $.identifier),
      optional($.where_clause),
      $._style_properties,
    ),

    nodes_style: $ => seq(
      'nodes',
      optional($.where_clause),
      $._style_properties,
    ),

    edges_style: $ => seq(
      'edges',
      optional($.where_clause),
      $._style_properties,
    ),

    _style_properties: $ => seq(
      '{',
      optionalCommaSep($.attribute),
      '}',
    ),

    render_clause: $ => seq(
      'render',
      commaSep1(field('format', $.identifier)),
      optional(seq(
        '{',
        optionalCommaSep($.attribute),
        '}',
      )),
    ),
  },
});
