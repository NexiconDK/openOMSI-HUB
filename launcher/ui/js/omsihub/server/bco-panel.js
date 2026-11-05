// Omsi-Hub (NexiconDK, 1 November 2026): Bus Company Online's main server administered - one panel
// for two places, as the game servers' (`panel.js`, whose pieces it uses): the main server's door
// (`GET /admin` on its api_port: crates/omsi-launcher-core/src/company/server_admin.rs serves this
// file as `/admin/bco-panel.js`) and the launcher's Multiplayer → My server.
//
//   api.get(path)          → `/admin/api/<path>` (overview, worlds, companies, company?id=, players,
//                            market, log?from= | ?world= | ?company=)
//   api.post(path, body)   → command (a console line), player, tender, stop
//   api.config()           → { text, path }       api.saveConfig(text)
//
// `opts.tr` translates, `opts.onLost` is told when the server no longer answers, `opts.open`
// opens an address (the launcher's: in the browser), `opts.host` is the machine the worlds' own
// pages are on (the launcher's: 127.0.0.1; the browser's: the page's), `opts.stop` false hides
// "Stop".

import { h, addCss, sureButton, settingsView } from './panel.js';

const BOOL = 'bool';
/** company-server.cfg's keys as the form shows them: [key, label, kind, help]. */
export const BCO_FIELDS = [
  ['General'],
  ['name', 'Name', 'text', "Shown in the players' launchers"],
  ['root', 'OMSI folder', 'text', 'The folder with the maps; empty: the one the launcher on this machine uses'],
  ['Clock'],
  ['real_time', "The machine's real date and time", BOOL, 'Off: the date and time below at the very first start, then 1:1 with the real clock'],
  ['date', 'First date', 'text', 'YYYY-MM-DD, empty: today'],
  ['time', 'First time', 'text', 'HH:MM'],
  ["The maps' worlds"],
  ['game', 'Game program', 'text', 'Empty: the one beside the main server'],
  ['first_port', 'First port', 'number', 'Each world takes two: UDP and its web port'],
  ['max_worlds', 'Worlds at once', 'number'],
  ['world_idle', 'Minutes without players', 'number', 'A world stops this long after its last player (when not kept running)'],
  ['keep_worlds', 'Keep worlds running', BOOL, 'While the machine can carry them'],
  ['max_memory', 'Most memory in use (%)', 'number'],
  ['max_cpu', 'Most processor in use (%)', 'number'],
  ['traffic', 'Traffic', 'number', 'Random cars in each world'],
  ['passengers', 'Passengers', BOOL],
  ['max_players', 'Most players a world', 'number'],
  ['share_positions', 'Live map for everyone', BOOL],
  ["The players' door"],
  ['api_port', 'Door port', 'number', "The players' launchers come here - and this panel"],
  ['first_company_port', "The companies' first port", 'number'],
  ['tunnel', 'Cloudflare tunnel', BOOL, 'An https address for the door, the companies and the worlds: players anywhere'],
  ['logins', 'Ways to log in', [['account steam', 'Omsi-Hub account and Steam'], ['account', 'Omsi-Hub account only'], ['steam', 'Steam only']]],
  ['Money'],
  ['price_level', 'Price level', [['dk', 'Denmark: a timetable hour about 1,000 kr.'], ['', "Each company's own country"], ['de', 'Germany'], ['nl', 'Netherlands'], ['se', 'Sweden'], ['no', 'Norway'], ['gb', 'United Kingdom']], "The companies' wages, fuel and buses, and the authority's estimate in the tenders"],
  ['start_money', 'A new player starts with', 'number', 'Whole units'],
  ['found_price', 'Founding a company costs', 'number', "Of the player's own money"],
  ['depot_price', 'A depot on another map', 'number', "Of the company's money"],
  ['bus_move_price', 'Moving a bus to another map', 'number'],
  ['bus_move_days', 'Days a moved bus is away', 'number'],
  ['big_purchase', "A depot's purchase that waits for the head office", 'number'],
  ['What founding asks'],
  ['found_trips', 'Trips driven first', 'number', '0: none'],
  ['found_course', 'The leadership course', BOOL],
  ['course_price', "The course's price", 'number'],
  ['course_days', "The course's days", 'number'],
  ['found_test', 'A driving test passed', BOOL],
  ['The tenders'],
  ['cap_per_hour', 'The cap a timetable hour (kroner)', 'number', "0: the authority's estimate by the factor below. Another number: the cap of every new tender, whatever the estimate"],
  ['cap_per_bus_day', 'The cap a contract bus a day (kroner)', 'number', "A bid is a price a timetable hour and an amount a contract bus a day. 0: the authority's estimate of a bus a day by the factor below"],
  ['tender_cap_factor', 'The cap: the estimate by', 'number', 'When the cap above is 0 (1.5: half again the estimate)'],
  ['tender_low_share', 'Abnormally low under (%)', 'number', "Of the authority's estimate: a bid below it is rejected"],
  ['tender_operators', 'Operators bidding in a tender', 'number', "Empty: as the rules draw them; 0: none - the map's companies alone"],
  ['tender_round_weeks', 'A round of new lines every (weeks)', 'number'],
  ['tender_lines_per_round', 'Lines in a round', 'number', 'Empty: the rules\' and one more a company'],
  ['tender_reserve', 'Reserve buses (%)', 'number', "Of a tender's contract buses: shown on the tender page, not paid (10: ten per cent, at least one from five buses)"],
  ['calendar', 'Calendar', [['map', "Each map's own"], ['dk', 'Denmark: school holidays and public holidays']], 'Which tours run on a day; your own holidays: Tenders and market'],
  ['change_must_accept', 'A change a company must run (%)', 'number', 'More timetable hours than this share: the company may decline within a day. Empty or 0: every change'],
  ['tender_award', 'How tenders are awarded', [['viborg', 'The Viborg model: 50 % price + 50 % quality'], ['lowest', 'The lowest price wins']], 'Quality: the operating quality 25 %, the vehicles 10 %, the implementation plan 10 %, the working environment 5 %'],
  ['doubling_price', 'Doubling: a timetable hour with a bus of no contract (kroner)', 'number', "An extra bus behind a full departure; one of the line's own buses standing free is paid at the contract's price"],
  ['doubling_full', 'Doubling: full at (% of the places)', 'number', 'Seats and standing: 100 - people left at the stop. A departure full on 3 of its last 5 like days is ordered'],
  ['Driving yourself'],
  ['player_pay_share', "A player's pay (%)", 'number', "Of what an AI driver costs the company for the same time (90: a player is cheaper than an AI driver)"],
  ['ai_quality_cap', 'An AI trip counts at most', 'number', 'In the operating quality of the tenders (0-100); a live trip that passes counts its judgement'],
  ['live_surcharge', 'Live surcharge (%)', 'number', "The authority's extra on the hour price for the hours driven live and passed; 0: none"],
  ['driver_pool', 'AI drivers a map', 'number', "Empty or 0: as many as the companies like. None left: agency drivers run their open duties at the agency's price"],
  ['agency_fallback', 'Agency drivers run the duties nobody covers', BOOL, 'Off: such a duty is dropped. Only the admin decides this: the companies cannot choose otherwise'],
  ['employment_notice_days', "Notice of a change of a driver's employment (days)", 'number', 'Full time, part time, reserve or hourly paid. 0: from the next day. From hourly paid to part or full time is always the next day'],
  ['rest_fine', 'A breach of the rest rules (kroner)', 'number', "RosterFlow Pro: a duty given anyway breaking 11 hours' rest, a day of 13 hours or a week of 48 costs the company this fine. 0: none, the driver is only less content"],
  ['urgent_bonus', 'Urgent tasks pay more (%)', 'number', 'A short-notice doubling the admin ordered (players only) or rail replacement: a player who drives it is paid this much more'],
  ['loyalty_bonus', 'A fixed duty pays more (%)', 'number', 'A duty a company gives a player for good on its weekday'],
  ['boss_duties', "The boss's duties a week", 'number', 'The owner or a co-owner driving this many duties live in a week raises the working environment by 10 points; 0: never'],
  ["The authority's fines (kroner)"],
  ['fine_dropped_minute', 'A journey not run, a minute', 'number'],
  ['fine_wrong_bus', 'A smaller bus than the timetable asks', 'number'],
  ['fine_diesel_on_electric', 'Diesel on an electric contract', 'number'],
  ['fine_wrong_fuel', 'A bus of a fuel the tender does not allow', 'number'],
  ['fine_wrong_floor', 'A bus of a floor the tender does not allow', 'number'],
  ['fine_inspection', 'A fault the weekly inspection finds', 'number', 'A dirty bus, a damaged one, no passenger information where the tender asks for it'],
  ['fine_early', 'Leaving a stop early', 'number'],
  ['fine_late', 'Late without telling the control room', 'number'],
  ['fine_driving', 'A complaint about the driving', 'number'],
  ['fine_temperature', 'A complaint about the temperature', 'number'],
  ['fine_route', 'A complaint about the route', 'number'],
  ['fine_ticket', 'A ticket sold wrong', 'number'],
  ['fine_red_light', 'A red light run', 'number'],
  ['Administration'],
  ['admin_password', 'Admin password', 'password', 'At least 8 characters - this panel and every world\'s'],
  ['web_admin', 'Admin panel from elsewhere', BOOL, 'Off: only on this machine. On: from anywhere the door is reached, the tunnel too, with the admin password'],
];

/** What the main server takes for a key company-server.cfg leaves out (`main_server::MainCfg::parse`). */
export const BCO_DEFAULTS = {
  name: 'Bus Company Online', real_time: '1', time: '06:00', first_port: '27015', max_worlds: '4', world_idle: '10', keep_worlds: '1', max_memory: '85', max_cpu: '90',
  traffic: '30', passengers: '1', max_players: '16', share_positions: '1', api_port: '27040', first_company_port: '27045', tunnel: '0', logins: 'account steam',
  price_level: 'dk', player_pay_share: '90', ai_quality_cap: '70', live_surcharge: '5', agency_fallback: '1', employment_notice_days: '2', rest_fine: '0', urgent_bonus: '50', loyalty_bonus: '5', boss_duties: '2', start_money: '20000', found_price: '10000', depot_price: '250000', bus_move_price: '5000', bus_move_days: '2', big_purchase: '500000',
  found_trips: '20', found_course: '1', course_price: '5000', course_days: '3', found_test: '1', cap_per_hour: '0', cap_per_bus_day: '0', tender_cap_factor: '1.5', tender_low_share: '85', tender_round_weeks: '4', tender_award: 'viborg', calendar: 'map', doubling_price: '650', doubling_full: '100', tender_reserve: '10', fine_dropped_minute: '20.83',
  fine_wrong_bus: '2500', fine_diesel_on_electric: '2500', fine_wrong_fuel: '2500', fine_wrong_floor: '2500', fine_inspection: '1000', fine_early: '2500', fine_late: '2500', fine_driving: '2000', fine_temperature: '1500',
  fine_route: '2500', fine_ticket: '2500', fine_red_light: '2500', web_admin: '0',
};

/** (3 November 2026) What a tender may ask of the buses (`server_buses::FloorRule`, `FuelRule`). */
const FLOOR_RULES = [['low_or_entry', 'Low floor or low entry'], ['low_only', 'Low floor only'], ['any', 'High floor allowed']];
const FUEL_RULES = [['any', 'Any fuel (diesel on HVO, CNG or electric)'], ['cng_or_electric', 'CNG or electric'], ['electric', 'Electric only (zero emission)']];

export const BCO_FORM = { fields: BCO_FIELDS, defaults: BCO_DEFAULTS, title: 'Main server settings', reading: 'Reading company-server.cfg…', asText: 'Edit company-server.cfg as text' };

const CSS = `
.bco-alert{display:flex;gap:10px;align-items:flex-start;padding:8px 10px;border-radius:8px;background:var(--sp-field)}
.bco-alert>b{display:inline-block;width:8px;height:8px;border-radius:50%;margin-top:6px;flex:none;background:var(--sp-accent)}
.bco-alert.warn>b{background:var(--sp-danger)}
.bco-code{font:600 15px ui-monospace,Consolas,monospace;letter-spacing:.04em;word-break:break-all}
.bco-tag{display:inline-block;padding:1px 8px;border-radius:99px;font-size:11px;font-weight:700;background:var(--sp-field);color:var(--sp-dim);margin-left:6px;white-space:nowrap}
.bco-tag.ok{background:rgba(76,183,130,.16);color:var(--sp-ok)}
.bco-tag.warn{background:rgba(232,160,48,.16);color:var(--sp-accent)}
.bco-tag.bad{background:rgba(229,83,75,.16);color:var(--sp-danger)}
.bco-neg{color:var(--sp-danger)}
.bco-pick{cursor:pointer}
.bco-pick:hover td{background:rgba(255,255,255,.03)}
.bco-pick.on td{background:rgba(232,160,48,.08)}
.bco-out{background:#0d0f12;color:#cfd3da;border-radius:10px;padding:10px;font:12px/1.5 ui-monospace,Consolas,monospace;max-height:260px;overflow:auto;white-space:pre-wrap;margin:0}
.bco-bids{margin:4px 0 0;padding-left:18px;color:var(--sp-dim);font-size:12px}
.bco-sub{font-size:12px;color:var(--sp-dim)}
`;

const money = (n) => (n == null ? '-' : Math.round(n).toLocaleString());
const message = (e) => String(e?.message || e);
const ago = (secs) => {
  if (!secs) return '-';
  const d = new Date(secs * 1000);
  return d.toLocaleDateString() + ' ' + d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
};

export function mountPanel(root, api, opts = {}) {
  addCss();
  if (!document.getElementById('bco-css')) document.head.append(h('style', { id: 'bco-css' }, CSS));
  const tr = opts.tr || ((s) => s);
  const trf = (s, vars) => Object.entries(vars).reduce((t, [k, v]) => t.replaceAll(`%{${k}}`, v), tr(s));
  const host = opts.host || (typeof location !== 'undefined' && location.hostname) || '127.0.0.1';
  const TABS = [
    ['overview', 'Overview'], ['worlds', 'Worlds'], ['companies', 'Companies'], ['players', 'Players'],
    ['market', 'Tenders and market'], ['buses', 'Buses'], ['competitors', 'Competitors'], ['settings', 'Rules and economy'], ['log', 'Log'],
  ];
  let tab = 'overview';
  const data = {};
  // (5 November 2026: why a tab's question failed, by its name)
  const failed = {};
  const asking = (w) => (failed[w] ? trf('The main server could not answer: %{why}', { why: failed[w] }) : tr('Asking the main server…'));
  let lost = false;
  let timer = null;
  let picked = { company: null, player: null };
  let companyInfo = null;
  let logSrc = 'main';
  let logFrom = 0;
  let logLines = [];
  let follow = true;
  let busy = false;

  const said = h('div', { class: 'sp-say' });
  const say = (text, bad) => {
    said.textContent = text;
    said.style.color = bad ? 'var(--sp-danger)' : '';
  };
  /** A console line (`main_server::command`): its answer said, the tab asked again. */
  const run = async (line, done) => {
    try {
      const r = await api.post('command', line);
      say(done || r?.answer || tr('Done'));
      setTimeout(refresh, 600);
      return r?.answer;
    } catch (e) {
      say(message(e), true);
      return null;
    }
  };
  const post = async (path, body, done) => {
    try {
      const r = await api.post(path, JSON.stringify(body));
      say(done || r?.answer || tr('Done'));
      setTimeout(refresh, 600);
      return true;
    } catch (e) {
      say(message(e), true);
      return false;
    }
  };
  const open = (url) => {
    if (opts.open) opts.open(url);
    else window.open(url, '_blank', 'noopener');
  };

  const el = h('div', { class: 'sp' });
  root.replaceChildren(el);
  const tabBar = h('div', { class: 'sp-tabs' });
  const body = h('div', { style: 'display:flex;flex-direction:column;gap:12px' });
  el.append(tabBar, body, said);
  const settings = settingsView({ config: api.config, saveConfig: api.saveConfig }, tr, say, BCO_FORM);

  const fact = (value, label, sub) => h('div', { class: 'sp-fact' }, h('b', {}, value), h('span', {}, label), sub ? h('div', { class: 'bco-sub' }, sub) : null);
  const tag = (text, kind = '') => h('span', { class: 'bco-tag ' + kind }, text);
  // (3 November 2026) a tender's requirements of the buses in words
  const ruleOf = (list, key) => tr((list.find((x) => x[0] === key) || [key, key])[1]);
  const needsText = (n) => [ruleOf(FLOOR_RULES, n.floor), ruleOf(FUEL_RULES, n.fuel), n.max_age ? trf('buses at most %{n} years', { n: n.max_age }) : null, n.screens ? tr('screens') : null].filter(Boolean).join(' · ');
  const card = (title, ...kids) => h('div', { class: 'sp-card' }, title ? h('h3', {}, title) : null, ...kids);
  const empty = (text) => h('div', { class: 'sp-empty' }, text);
  const table = (heads, rows) => h('div', { style: 'overflow-x:auto' }, h('table', { class: 'sp-table' }, h('tr', {}, ...heads.map((x) => (Array.isArray(x) ? h('th', { class: x[1] }, x[0]) : h('th', {}, x)))), ...rows));
  const btn = (label, onClick, cls = 'sp-btn small') => h('button', { class: cls, type: 'button', onClick }, label);

  function drawTabs() {
    const o = data.overview;
    const n = { players: o?.online || 0, worlds: o?.worlds?.running || 0, companies: o?.companies || 0 };
    tabBar.replaceChildren(...TABS.map(([t, label]) => h('button', { class: 'sp-tab' + (t === tab ? ' on' : ''), type: 'button', onClick: () => { tab = t; draw(); refresh(); } }, tr(label), n[t] ? h('span', { class: 'sp-n' }, n[t]) : null)));
  }

  // --- 1. overview ------------------------------------------------------------------------------

  function alertText(a) {
    if (a.kind === 'red') return trf('%{name} is in the red: %{money}', { name: a.name, money: money(a.money) });
    if (a.kind === 'loading') return trf('The world of %{name} has been loading for %{n} min', { name: a.name, n: a.minutes }) + (a.what ? ` (${a.what})` : '');
    if (a.kind === 'closing') return trf('%{n} tenders of %{name} close today', { n: a.n, name: a.name });
    return a.text || '';
  }

  function overview() {
    const o = data.overview;
    if (!o) return [card(null, empty(tr('Asking the main server…')))];
    const head = card(null,
      h('div', { class: 'sp-head' },
        h('h2', {}, o.name || 'Bus Company Online'),
        h('span', { class: 'sp-chip' + (lost ? ' off' : '') }, h('i'), tr(lost ? 'Not answering' : 'Running')),
        o.real_time ? tag(tr('Real time'), 'ok') : null),
      h('div', { class: 'sp-facts' },
        fact(o.time, o.date),
        fact(`${o.online} / ${o.players}`, tr('Players online'), tr('logged in from their launchers today')),
        fact(o.companies, tr('Companies'), o.groups ? trf('%{n} groups', { n: o.groups }) : null),
        fact(`${o.worlds.running} / ${o.worlds.max}`, tr('Worlds running'), o.worlds.loading ? trf('%{n} loading', { n: o.worlds.loading }) : o.worlds.keep ? tr('kept running') : null),
        o.machine ? fact(o.machine, tr('The machine')) : null),
    );
    const code = card(tr("The server's code"),
      o.code ? h('div', { class: 'sp-row' }, h('span', { class: 'bco-code sp-grow' }, o.code), btn(tr('Copy'), () => navigator.clipboard?.writeText(o.code).then(() => say(tr('Copied')), () => {}))) : h('div', { class: 'sp-dim' }, tr("The players' door is not open: see the log.")),
      h('div', { class: 'sp-dim' }, trf('Players enter it in Bus Company Online in their launchers. The door is on port %{port}.', { port: o.api_port })),
      o.tunnel ? h('div', {}, tr('On the internet: '), h('a', { href: o.tunnel, target: '_blank', style: 'color:var(--sp-accent)', onClick: (e) => { e.preventDefault(); open(o.tunnel); } }, o.tunnel)) : null,
    );
    const alerts = card(tr('Wants looking at'),
      o.alerts?.length
        ? o.alerts.map((a) => h('div', { class: 'bco-alert ' + (a.level || '') }, h('b'), h('div', { class: 'sp-grow' }, alertText(a)),
          a.company ? btn(tr('Open'), () => { tab = 'companies'; picked.company = a.company; companyInfo = null; draw(); refresh(); }) : null))
        : h('div', { class: 'sp-dim' }, tr('Nothing wants looking at.')),
    );
    const clock = card(tr("The server's clock"),
      h('div', { class: 'sp-dim' }, o.real_time ? tr("It runs on this machine's real time.") : tr('It runs 1:1 with the real clock from its first date and time.')),
      h('div', { class: 'sp-row' },
        btn(tr('Compare with the machine'), () => run('clock')),
        o.real_time ? sureButton(tr("Set to the machine's time now"), tr('Sure?'), () => run('clock now'), 'sp-btn small') : null),
    );
    const stop = opts.stop !== false
      ? h('div', { class: 'sp-row' }, sureButton(tr('Stop the main server'), tr('Sure? The worlds stop, every company is saved'), () => post('stop', {}, tr('The main server stops: the worlds stop and every company is saved.')), 'sp-btn danger'))
      : null;
    return [head, h('div', { class: 'sp-grid' }, code, alerts, clock), stop];
  }

  // --- 2. the worlds ----------------------------------------------------------------------------

  const worldUrl = (w, path) => `http://${host}:${w.web_port}${path}`;

  function worlds() {
    const d = data.worlds;
    if (!d) return card(tr('Worlds'), empty(asking('worlds')));
    if (!d.maps.length) return card(tr('Worlds'), empty(tr('No map is installed in the OMSI folder of the settings.')));
    const rows = d.maps.map((m) => {
      const w = m.world;
      let state;
      if (!w) state = tag(tr('Stopped'));
      else if (w.running) state = h('span', {}, tag(tr('Running'), 'ok'), w.empty_minutes != null ? h('div', { class: 'bco-sub' }, trf('no players for %{n} min', { n: w.empty_minutes })) : null);
      else state = h('span', {}, tag(tr('Loading'), 'warn'), h('div', { class: 'bco-sub' }, [w.loading, trf('%{n} min', { n: w.minutes })].filter(Boolean).join(' · ')));
      const players = w?.status?.players || [];
      const acts = h('div', { class: 'sp-row', style: 'justify-content:flex-end' });
      if (!w) acts.append(btn(tr('Start'), () => run(`start ${m.folder}`), 'sp-btn small main'));
      else {
        acts.append(
          btn(tr('Admin'), () => open(w.status?.tunnel && host !== '127.0.0.1' && location.protocol === 'https:' ? w.status.tunnel + '/admin' : worldUrl(w, '/admin'))),
          btn(tr('Live map'), () => open(worldUrl(w, '/kort'))),
          btn(tr('Log'), () => { tab = 'log'; setLog('world:' + m.folder); }),
          sureButton(tr('Stop'), players.length ? trf('Sure? %{n} players are in it', { n: players.length }) : tr('Sure?'), () => run(`stop ${m.folder}`)),
        );
      }
      return h('tr', {},
        h('td', {}, h('b', {}, m.folder), m.companies.length ? h('div', { class: 'bco-sub' }, m.companies.join(', ')) : h('div', { class: 'bco-sub' }, tr('no company'))),
        h('td', {}, state),
        h('td', { class: 'sp-hide' }, w ? `UDP ${w.port} · web ${w.web_port}` : '-', w?.status?.code ? h('div', { class: 'bco-sub', title: tr("The world's code") }, w.status.code) : null),
        h('td', {}, players.length ? players.map((p) => h('div', {}, p.name, p.line ? h('span', { class: 'bco-sub' }, ` · ${p.line}`) : null)) : h('span', { class: 'sp-dim' }, '-')),
        h('td', { class: 'sp-acts' }, acts),
      );
    });
    return card(trf('Worlds - %{n} at once at most', { n: d.max }),
      table([tr('Map'), tr('World'), [tr('Ports'), 'sp-hide'], tr('Players'), ''], rows),
      h('div', { class: 'sp-dim' }, tr('A world starts by itself when a player of one of its companies comes; Start brings it up before.')));
  }

  // --- 3. the companies -------------------------------------------------------------------------

  function companies() {
    const d = data.companies;
    if (!d) return [card(tr('Companies'), empty(asking('companies')))];
    const list = d.companies;
    const rows = list.map((c) => h('tr', { class: 'bco-pick' + (picked.company === c.id ? ' on' : ''), onClick: () => { picked.company = picked.company === c.id ? null : c.id; companyInfo = null; draw(); refresh(); } },
      h('td', {}, h('b', {}, c.name), c.group ? tag(tr('Group head office')) : null, h('div', { class: 'bco-sub' }, c.id)),
      h('td', { class: 'sp-hide' }, c.map || '-'),
      h('td', {}, c.owner || h('span', { class: 'sp-dim' }, tr('nobody'))),
      h('td', { class: c.cash < 0 ? 'bco-neg' : '' }, money(c.cash), c.debt ? h('div', { class: 'bco-sub' }, trf('debt %{m}', { m: money(c.debt) })) : null),
      h('td', { class: 'sp-hide' }, `${c.lines} / ${c.buses} / ${c.staff}`),
      h('td', {}, c.runs ? tag(tr('Running'), 'ok') : tag(tr('Stopped'), 'bad')),
    ));
    const out = [card(tr('Companies'),
      list.length ? table([tr('Company'), [tr('Map'), 'sp-hide'], tr('Owner'), tr('Money'), [tr('Lines / buses / staff'), 'sp-hide'], tr('Process')], rows) : empty(tr('No company has been founded on the server yet.')),
      list.length ? h('div', { class: 'sp-dim' }, tr('Click a company for its details.')) : null)];
    if (picked.company) out.push(companyCard(list.find((c) => c.id === picked.company)));
    // a company founded by the server's admin
    const maps = (data.worlds?.maps || []).map((m) => m.folder);
    const name = h('input', { class: 'sp-grow', placeholder: tr("The company's name") });
    const map = h('select', {}, ...maps.map((m) => h('option', { value: m }, m)));
    const mode = h('select', {}, ...[['normal', 'Normal'], ['casual', 'Casual'], ['realistic', 'Realistic']].map(([v, l]) => h('option', { value: v }, tr(l))));
    const owner = h('select', {}, h('option', { value: '' }, tr('No owner yet')), ...(data.players?.players || []).map((p) => h('option', { value: p.name }, p.name)));
    out.push(card(tr('Found a company'), h('div', { class: 'sp-row' }, name, map, mode, owner,
      btn(tr('Found'), () => { const n = name.value.trim(); if (n && map.value) run(`found ${n}; ${map.value}; ${mode.value}; ${owner.value}`); }, 'sp-btn main'))));
    return out;
  }

  function companyCard(c) {
    if (!c) return null;
    const i = companyInfo?.id === c.id ? companyInfo : null;
    const amount = h('input', { type: 'number', min: 1, placeholder: tr('Amount'), style: 'width:120px' });
    const give = (sign) => {
      const n = Math.abs(Number(amount.value) || 0);
      if (n > 0) run(`cash ${c.id}; ${sign}${n}`, trf('%{name}: %{amount} booked as capital', { name: c.name, amount: sign + money(n) }));
    };
    const players = data.players?.players || [];
    const newOwner = h('select', {}, ...players.map((p) => h('option', { value: p.id, selected: p.id === c.owner_id }, p.name)));
    const ROLES = [['driver', 'Driver'], ['planner', 'Planner'], ['manager', 'Manager'], ['co_owner', 'Co-owner']];
    const members = (i?.members || []).map((m) => {
      const role = h('select', {}, ...ROLES.map(([v, l]) => h('option', { value: v, selected: v === m.role }, tr(l))));
      return h('tr', {},
        h('td', {}, h('b', {}, m.name), m.owner ? tag(tr('Owner'), 'ok') : null, m.banned ? tag(tr('Banned'), 'bad') : null),
        h('td', { class: 'sp-acts' }, h('div', { class: 'sp-row', style: 'justify-content:flex-end' }, role, btn(tr('Set'), () => run(`role ${c.id}; ${m.name}; ${role.value}`)))));
    });
    return card(c.name,
      h('div', { class: 'sp-facts' },
        fact(money(c.cash), tr('Money')),
        fact(i ? i.reputation : '-', tr('Reputation')),
        fact(c.date || '-', tr("The company's day")),
        fact(c.mode || '-', tr('Game mode')),
        fact(c.port || '-', tr('Port')),
      ),
      c.code ? h('div', {}, tr('Code: '), h('span', { class: 'bco-code' }, c.code)) : null,
      !c.runs ? h('div', { class: 'sp-dim' }, tr('Its process does not run: money, owner and roles change when it does.')) : null,
      h('h3', {}, tr('Money')),
      h('div', { class: 'sp-row' }, amount, btn(tr('Give'), () => give('+'), 'sp-btn small main'), sureButton(tr('Take'), tr('Sure?'), () => give('-'))),
      h('h3', {}, tr('Owner')),
      players.length ? h('div', { class: 'sp-row' }, newOwner, btn(tr('Make owner'), () => run(`owner ${c.id}; ${newOwner.value}`))) : h('div', { class: 'sp-dim' }, tr('No player has logged in yet.')),
      h('h3', {}, tr('Players and roles')),
      i ? (members.length ? table([tr('Player'), ''], members) : h('div', { class: 'sp-dim' }, tr('Nobody has joined the company yet.'))) : h('div', { class: 'sp-dim' }, tr('Asking…')),
      i?.concessions?.length ? h('h3', {}, tr('Concessions')) : null,
      i?.concessions?.length ? table([tr('Line'), tr('From'), tr('Until')], i.concessions.map((x) => h('tr', {}, h('td', {}, x.number || x.line), h('td', {}, x.from), h('td', {}, x.until)))) : null,
      i?.depots?.length ? h('h3', {}, tr('Depots')) : null,
      i?.depots?.length ? h('div', {}, i.depots.map((d) => h('div', {}, d.name, h('span', { class: 'bco-sub' }, ` · ${d.id}`)))) : null,
      h('div', { class: 'sp-row' }, btn(tr("The process's log"), () => { tab = 'log'; setLog('company:' + c.id); })),
    );
  }

  // --- 4. the players ---------------------------------------------------------------------------

  const playerFilter = h('input', { class: 'sp-grow', placeholder: tr('Find a player…') });
  playerFilter.addEventListener('input', () => draw());

  /** (3 November 2026) The installed buses by floor and fuel, each with its size: what holds on the
   * server (the game's guess or the admin's correction), corrected here (`server_admin::bus_do`). */
  function busesView() {
    const d = data.buses;
    if (!d || d.reading) return [card(tr('Buses'), h('div', { class: 'sp-dim' }, tr('Reading the installed buses - every bus file once…')))];
    const ch = d.choices || {};
    const label = (list, key) => tr(((list || []).find((x) => x[0] === key) || [key, key])[1]);
    const intro = card(tr('Buses'), h('div', { class: 'sp-dim' }, tr("The floor, fuel and size of every installed bus as the server takes them: the game guesses them from the bus's files and name, you correct them here. The tenders' requirements, the vehicle points and the authority's fines go by them; the companies take a change at their next step.")));
    const groups = [];
    for (const [fk] of ch.floors || []) {
      const ofFloor = d.buses.filter((b) => b.floor === fk);
      if (!ofFloor.length) continue;
      const parts = [];
      for (const [dk] of ch.drives || []) {
        const list = ofFloor.filter((b) => b.drive === dk).sort((a, b) => a.size.localeCompare(b.size) || a.name.localeCompare(b.name));
        if (!list.length) continue;
        const rows = list.map((b) => {
          // (what is chosen is kept over the tab's redraws until it is saved)
          const k = (x) => `bus:${b.file}:${x}`;
          const sel = (choices, value, key) => keep(k(key), h('select', {}, ...choices.map(([v, l]) => h('option', { value: v, selected: v === value }, tr(l)))));
          const floor = sel(ch.floors, b.floor, 'floor');
          const drive = sel(ch.drives, b.drive, 'drive');
          const size = sel(ch.sizes, b.size, 'size');
          const screens = h('input', { type: 'checkbox', checked: kept[k('screens')] ?? !!b.screens, title: tr('Passenger information screens'), onChange: (e) => (kept[k('screens')] = e.target.checked) });
          const forget = () => ['floor', 'drive', 'size', 'screens'].forEach((x) => delete kept[k(x)]);
          const set = Object.values(b.set || {}).some(Boolean);
          return h('tr', {},
            h('td', {}, h('b', {}, b.name), b.coach ? tag(tr('Coach')) : null, set ? tag(tr('corrected'), 'ok') : null, h('div', { class: 'bco-sub' }, b.file),
              set ? h('div', { class: 'bco-sub' }, trf('the game guesses: %{f} · %{d} · %{s}', { f: label(ch.floors, b.guess.floor), d: label(ch.drives, b.guess.drive), s: label(ch.sizes, b.guess.size) })) : null),
            h('td', {}, h('div', { class: 'sp-row', style: 'flex-wrap:wrap' }, floor, drive, size, h('label', { class: 'sp-row' }, screens, tr('screens')))),
            h('td', { class: 'sp-acts' }, h('div', { class: 'sp-row', style: 'justify-content:flex-end' },
              btn(tr('Save'), () => { post('bus', { file: b.file, floor: floor.value === b.guess.floor && !b.set.floor ? '' : floor.value, drive: drive.value === b.guess.drive && !b.set.drive ? '' : drive.value, size: size.value === b.guess.size && !b.set.size ? '' : size.value, screens: screens.checked || b.set.screens ? screens.checked : '' }); forget(); }, 'sp-btn small main'),
              set ? btn(tr('As the game guesses'), () => { post('bus', { file: b.file }); forget(); }) : null)));
        });
        parts.push(h('h4', {}, `${label(ch.drives, dk)} · ${list.length}`), table([tr('Bus'), tr('Floor · fuel · size'), ''], rows));
      }
      groups.push(card(`${label(ch.floors, fk)} · ${ofFloor.length}`, ...parts));
    }
    return [intro, ...(groups.length ? groups : [card(null, empty(tr('No bus is installed.')))])];
  }

  /** (3 November 2026) The server's competitors: the AI operators bidding on every map, each with
   * its size, style, buses and quality (`server_admin::competitor_do`); without them the maps'
   * drawn operators, to take over. */
  let editing = null;
  function competitorsView() {
    const d = data.competitors;
    if (!d) return [card(tr('Competitors'), empty(asking('competitors')))];
    const ch = d.choices || {};
    const label = (list, key) => tr(((list || []).find((x) => x[0] === key) || [key, key])[1]);
    const intro = card(tr('Competitors'), h('div', { class: 'sp-dim' }, tr("The AI operators that bid in the server's tenders, on every map. The size says which packages one bids on and how many contracts it holds at once on a map; the style what it puts on its own cost, never under the abnormally low line. One bids only with buses that meet a tender's requirements. Its quality rises a point with each contract run to its term and falls five with each given up. Without a list every map keeps its own drawn operators.")));
    // the form: a new one (id 0) or the one being changed
    const form = (c) => {
      const k = (x) => `comp:${c ? c.id : 0}:${x}`;
      const name = keep(k('name'), h('input', { value: c ? c.name : '', placeholder: tr('Name') }));
      const size = keep(k('size'), h('select', {}, ...(ch.sizes || []).map(([v, l, most]) => h('option', { value: v, selected: v === (c ? c.size : 'medium') }, trf('%{size}: at most %{n} contracts a map', { size: tr(l), n: most })))));
      const style = keep(k('style'), h('select', {}, ...(ch.styles || []).map(([v, l, a, b]) => h('option', { value: v, selected: v === (c ? c.style : 'normal') }, trf('%{style}: +%{a} to %{b} % over its own cost', { style: tr(l), a, b })))));
      const quality = keep(k('quality'), h('input', { type: 'number', min: 0, max: 100, value: c ? c.quality : ch.quality, style: 'width:70px' }));
      const box = (list, chosen, key) => (list || []).map(([v, l]) => {
        const id = k(key + ':' + v);
        const input = h('input', { type: 'checkbox', checked: kept[id] ?? chosen.includes(v), onChange: (e) => (kept[id] = e.target.checked) });
        return h('label', { class: 'sp-row' }, input, tr(l));
      });
      const fuels = box(ch.drives, c ? c.fuels : ['diesel'], 'fuel');
      const floors = box(ch.floors, c ? c.floors : ['low_floor', 'low_entry'], 'floor');
      const values = (list, els) => (list || []).map((x) => x[0]).filter((_, n) => els[n].querySelector('input').checked);
      const forget = () => Object.keys(kept).filter((x) => x.startsWith(`comp:${c ? c.id : 0}:`)).forEach((x) => delete kept[x]);
      const save = () => { post('competitor', { do: 'put', id: c ? c.id : 0, name: name.value, size: size.value, style: style.value, quality: quality.value, fuels: values(ch.drives, fuels), floors: values(ch.floors, floors) }); forget(); editing = null; };
      return h('div', { class: 'sp-col', style: 'gap:8px' },
        h('div', { class: 'sp-row', style: 'flex-wrap:wrap' }, name, size, style, h('label', { class: 'sp-row' }, h('span', { class: 'sp-dim' }, tr('Quality')), quality)),
        h('div', { class: 'sp-row', style: 'flex-wrap:wrap' }, h('span', { class: 'sp-dim', style: 'width:70px' }, tr('Fuels')), ...fuels),
        h('div', { class: 'sp-row', style: 'flex-wrap:wrap' }, h('span', { class: 'sp-dim', style: 'width:70px' }, tr('Floors')), ...floors),
        h('div', { class: 'sp-row' }, btn(c ? tr('Save') : tr('Add a competitor'), save, 'sp-btn small main'), c ? btn(tr('Cancel'), () => { forget(); editing = null; draw(); }) : null));
    };
    const rows = (d.competitors || []).map((c) => h('tr', {},
      h('td', {}, h('b', {}, c.name)),
      h('td', {}, tag(label(ch.sizes, c.size)), ' ', tag(label(ch.styles, c.style), c.style === 'aggressive' ? 'warn' : c.style === 'careful' ? 'ok' : '')),
      h('td', {}, c.fuels.map((f) => label(ch.drives, f)).join(', '), h('div', { class: 'bco-sub' }, c.floors.map((f) => label(ch.floors, f)).join(', '))),
      h('td', {}, trf('%{n} of %{most} a map', { n: c.contracts, most: c.most })),
      h('td', {}, String(c.quality)),
      h('td', { class: 'sp-acts' }, h('div', { class: 'sp-row', style: 'justify-content:flex-end' },
        btn(tr('Change'), () => { editing = editing === c.id ? null : c.id; draw(); }),
        sureButton(tr('Delete'), tr('Sure? Its contracts run out'), () => post('competitor', { do: 'delete', id: c.id }), 'sp-btn small danger')))));
    const list = card(tr('The competitors'),
      rows.length ? table([tr('Name'), tr('Size · style'), tr('Buses'), tr('Contracts'), tr('Quality'), ''], rows) : empty(tr('No competitor made yet: every map keeps its own drawn operators.')),
      editing != null ? form((d.competitors || []).find((c) => c.id === editing)) : null);
    const add = card(tr('Add a competitor'), form(null));
    const drawn = (d.drawn || []).filter((m) => m.operators.length);
    const old = drawn.length ? card(tr("The maps' drawn operators"),
      h('div', { class: 'sp-dim' }, tr('As the game drew them for each map. Take them over to make them competitors you can change: by their character, with diesel buses of low floor and low entry.')),
      ...drawn.map((m) => h('div', {}, h('h4', {}, m.map), table([tr('Name'), tr('Kind'), tr('Contracts'), tr('Quality')], m.operators.map((o) => h('tr', {}, h('td', {}, o.name), h('td', {}, tr(o.kind)), h('td', {}, String(o.contracts)), h('td', {}, String(o.quality))))))),
      h('div', { class: 'sp-row' }, btn(tr('Take them over as competitors'), () => post('competitor', { do: 'take_over' }), 'sp-btn small main'))) : null;
    return [intro, list, add, old];
  }

  function players() {
    const d = data.players;
    if (!d) return [card(tr('Players'), empty(asking('players')))];
    const f = playerFilter.value.trim().toLowerCase();
    const list = d.players.filter((p) => !f || p.name.toLowerCase().includes(f) || p.id.includes(f));
    const rows = list.map((p) => h('tr', { class: 'bco-pick' + (picked.player === p.id ? ' on' : ''), onClick: () => { picked.player = picked.player === p.id ? null : p.id; draw(); } },
      h('td', {}, h('b', {}, p.name), p.online ? tag(tr('Online'), 'ok') : null, p.banned ? tag(tr('Banned'), 'bad') : null, h('div', { class: 'bco-sub' }, p.id)),
      h('td', { class: 'sp-hide' }, [p.account ? tr('Account') : null, p.steam ? 'Steam' : null].filter(Boolean).join(' + ') || '-'),
      h('td', {}, money(p.money)),
      h('td', { class: 'sp-hide' }, p.owns.length ? p.owns.join(', ') : '-'),
      h('td', { class: 'sp-hide' }, ago(p.seen)),
    ));
    const out = [card(trf('Players - %{n}', { n: d.players.length }), h('div', { class: 'sp-row' }, playerFilter),
      d.players.length ? table([tr('Player'), [tr('Logs in with'), 'sp-hide'], tr('Money'), [tr('Owns'), 'sp-hide'], [tr('Last seen'), 'sp-hide']], rows) : empty(tr('No player has logged in yet.')))];
    const p = d.players.find((x) => x.id === picked.player);
    if (p) out.push(playerCard(p));
    return out;
  }

  function playerCard(p) {
    const amount = h('input', { type: 'number', min: 1, placeholder: tr('Amount'), style: 'width:120px' });
    const pay = (sign) => {
      const n = Math.abs(Number(amount.value) || 0);
      if (n > 0) run(`money ${p.id}; ${sign}${n}`);
    };
    const exact = h('input', { type: 'number', min: 0, placeholder: tr('New balance'), style: 'width:120px' });
    const trips = h('input', { type: 'number', min: 0, value: p.trips, style: 'width:90px' });
    const cls = h('select', {}, ...[['midi', 'Midibus'], ['solo', 'Solo bus'], ['articulated', 'Articulated bus'], ['double', 'Double-decker']].map(([v, l]) => h('option', { value: v }, tr(l))));
    const why = h('input', { class: 'sp-grow', placeholder: tr('Why (he is told)'), maxlength: 200 });
    return card(p.name,
      h('div', { class: 'sp-facts' },
        fact(money(p.money), tr('Own money')),
        fact(p.trips, tr('Trips driven')),
        fact(p.course ? tr('Done') : tr('No'), tr('Leadership course')),
        fact(p.tests.length ? p.tests.join(', ') : '-', tr('Driving tests')),
        fact(p.keys, tr('Computers with a key')),
        fact(ago(p.since), tr('First came')),
      ),
      p.banned ? h('div', { class: 'bco-alert warn' }, h('b'), h('div', {}, trf('Banned: %{why}', { why: p.banned }))) : null,
      h('h3', {}, tr('Money')),
      h('div', { class: 'sp-row' }, amount, btn(tr('Give'), () => pay('+'), 'sp-btn small main'), btn(tr('Take'), () => pay('-')),
        exact, btn(tr('Set'), () => { if (exact.value !== '') run(`money ${p.id}; ${Math.max(0, Number(exact.value) || 0)}`); })),
      h('h3', {}, tr('What founding asks')),
      h('div', { class: 'sp-row' }, h('span', { class: 'sp-dim' }, tr('Trips')), trips, btn(tr('Set'), () => run(`grant ${p.id}; trips ${Math.max(0, Number(trips.value) || 0)}`)),
        btn(tr('Course done'), () => run(`grant ${p.id}; course`)),
        cls, btn(tr('Test passed'), () => run(`grant ${p.id}; test ${cls.value}`))),
      h('h3', {}, tr('Access')),
      p.banned
        ? h('div', { class: 'sp-row' }, btn(tr('Let him in again'), () => post('player', { id: p.id, do: 'unban' }), 'sp-btn small main'))
        : h('div', { class: 'sp-row' }, why, sureButton(tr('Ban'), tr('Sure? He cannot log in'), () => post('player', { id: p.id, do: 'ban', reason: why.value.trim() }))),
      h('div', { class: 'sp-row' }, sureButton(tr("Forget his computers' keys"), tr('Sure?'), () => post('player', { id: p.id, do: 'keys' }), 'sp-btn small'),
        h('span', { class: 'sp-dim' }, tr('A computer of his gets a new key at its next login with Steam (a lost or stolen PC).'))),
    );
  }

  // --- 5. the tenders and the market ------------------------------------------------------------

  const local = (s) => (s || '').replace(' ', 'T');
  // (3 November 2026) what is typed or chosen in the tab's forms, kept over its redraws (every
  // few seconds): by a key of the map and the field
  const kept = {};
  const keep = (key, el) => {
    const multi = el.tagName === 'SELECT' && el.multiple;
    if (kept[key] != null) {
      if (multi) [...el.options].forEach((o) => (o.selected = kept[key].includes(o.value)));
      else el.value = kept[key];
    }
    const save = () => (kept[key] = multi ? [...el.selectedOptions].map((o) => o.value) : el.value);
    el.addEventListener('input', save);
    el.addEventListener('change', save);
    return el;
  };

  function market() {
    const d = data.market;
    if (!d) return [card(tr('Tenders and market'), empty(asking('market')))];
    if (!d.maps.length) return [card(tr('Tenders and market'), empty(tr("No map has a market yet: it starts with a map's first company.")))];
    const kr = (n) => (n == null ? '-' : `${Math.round(n).toLocaleString()} kr.`);
    // (3 November 2026: the tenders' rules as they stand - changed under Rules and economy)
    const r = d.rules || {};
    const rules = card(tr('The tenders\' rules'),
      h('div', { class: 'sp-facts' },
        fact(r.cap_hour ? kr(r.cap_hour) : trf('estimate × %{f}', { f: r.cap_factor }), tr('The cap a timetable hour'), r.cap_hour ? tr('for every new tender') : tr("the authority's estimate by the factor")),
        fact(r.cap_bus_day ? kr(r.cap_bus_day) : trf('estimate × %{f}', { f: r.cap_factor }), tr('The cap a contract bus a day'), r.cap_bus_day ? tr('for every new tender') : tr("the authority's estimate by the factor")),
        fact(`${r.low_share} %`, tr('Abnormally low under'), tr("of the authority's estimate")),
        fact(r.operators == null ? tr('as the rules draw them') : String(r.operators), tr('Operators in a tender')),
        fact(trf('%{n} weeks', { n: r.round_weeks }), tr('A round of new lines'), r.per_round == null ? tr('the rules\' number of lines') : trf('%{n} lines', { n: r.per_round })),
        // (3 November 2026: how they are awarded, their days, the contracts' and the option)
        fact(tr(r.award === 'lowest' ? 'The lowest price wins' : 'The Viborg model: 50 % price + 50 % quality'), tr('Award')),
        fact(trf('%{a} + %{b} + %{c} days', { a: r.prep, b: r.days, c: r.start }), tr('A tender'), tr('to prepare, to bid, to the start')),
        fact(trf('%{a} / %{b} / %{c} days before', { a: (r.renew || [])[0], b: (r.renew || [])[1], c: (r.renew || [])[2] }), tr('A renewal'), tr('announced, opens, decided')),
        fact((r.lengths || []).map((x) => `${x[0]}/${x[1]}`).join(' · '), tr('Contract days, diesel/electric'), tr('small · medium · large work: under 1,000, up to 3,000, over 3,000 timetable hours a week')),
        fact(trf('%{n} × %{d} days', { n: r.options, d: r.option_days }), tr('Option'), tr('when run satisfactorily: not red, at most 2 % of the journeys dropped')),
        fact(r.doubling_price != null ? kr(r.doubling_price) : '-', tr('Doubling a timetable hour'), trf('with a bus of no contract · full at %{p} % of the places', { p: r.doubling_full })),
        // (3 November 2026: driving yourself pays - K1-K4)
        fact(`${r.player_pay} %`, tr("A player's pay"), tr("of what an AI driver costs for the same time")),
        fact(String(r.ai_cap), tr('An AI trip counts at most'), tr('in the operating quality: a live trip that passes counts its judgement')),
        fact(`${r.live_surcharge} %`, tr('Live surcharge'), tr('on the hour price, for the hours driven live')),
        fact(r.driver_pool ? String(r.driver_pool) : tr('as many as they like'), tr('AI drivers a map'), tr('none left: agency drivers run the open duties'))),
      h('div', { class: 'sp-dim' }, tr('Changed under Rules and economy: a new tender takes them; one open keeps its own (its cap can be set here).')));
    // (3 November 2026: the server's calendar and the admin's own holidays - school holidays on top)
    const hName = keep('holiday:name', h('input', { placeholder: tr('e.g. Strike'), style: 'width:160px' }));
    const hFrom = keep('holiday:from', h('input', { type: 'date', style: 'width:150px' }));
    const hUntil = keep('holiday:until', h('input', { type: 'date', style: 'width:150px' }));
    const holidays = card(tr('Holidays'),
      h('div', { class: 'sp-dim' }, d.calendar === 'dk' ? tr("The maps run by Denmark's school holidays and public holidays (Rules and economy: Calendar).") : tr("The maps run by their own calendars (Rules and economy: Calendar).")),
      (d.holidays || []).length ? table([tr('Holidays'), tr('From'), tr('Until'), ''], d.holidays.map((p) => h('tr', {}, h('td', {}, h('b', {}, p.name)), h('td', {}, p.from), h('td', {}, p.until || p.from),
        h('td', { class: 'sp-acts' }, sureButton(tr('Take away'), tr('Sure?'), () => post('events', { do: 'holiday_remove', id: p.id })))))) : null,
      h('div', { class: 'sp-row', style: 'flex-wrap:wrap' }, hName, hFrom, hUntil,
        btn(tr('Add the holidays'), () => post('events', { do: 'holiday_add', name: hName.value, from: hFrom.value, until: hUntil.value }), 'sp-btn small main')),
      h('div', { class: 'sp-dim' }, tr('As school holidays: the tours of the school holidays run, a contract is paid the timetable hours run and its buses every day.')));
    // (3 November 2026: force majeure - snow, a storm or a strike: no fines for what is dropped, H5)
    const kinds = [['snow', 'Snow'], ['storm', 'Storm'], ['strike', 'Strike']];
    const fmKind = keep('fm:kind', h('select', {}, ...kinds.map(([v, l]) => h('option', { value: v }, tr(l)))));
    const fmFrom = keep('fm:from', h('input', { type: 'date', style: 'width:150px' }));
    const fmUntil = keep('fm:until', h('input', { type: 'date', style: 'width:150px' }));
    const fmMap = keep('fm:map', h('select', {}, h('option', { value: '' }, tr('Every map')), ...d.maps.map((m) => h('option', { value: m.key }, m.map))));
    const everyone = [...new Map(d.maps.flatMap((m) => m.companies || []).map((c) => [c.id, c])).values()];
    const fmCompany = keep('fm:company', h('select', {}, h('option', { value: '' }, tr('Every company')), ...everyone.map((c) => h('option', { value: c.id }, c.name))));
    const fmWeather = keep('fm:weather', h('select', {}, h('option', { value: '' }, tr('As its name says (snow, storm)')), ...(d.weather_files || []).map((f) => h('option', { value: f }, f.replace(/^Weather\//, '')))));
    const kindName = (k) => tr((kinds.find((x) => x[0] === k) || [k, k])[1]);
    const majeure = card(tr('Force majeure'),
      h('div', { class: 'sp-dim' }, tr('Snow or a storm: every world of the map takes its weather, the buses run late and fewer ride. A strike: the duties of the company (or every company) do not run. What is dropped then is not fined, nor off the quality bonus - nor paid.')),
      (d.force_majeure || []).length ? table([tr('Kind'), tr('From'), tr('Until'), tr('Where'), ''], d.force_majeure.map((f) => h('tr', {}, h('td', {}, h('b', {}, kindName(f.kind))), h('td', {}, f.from), h('td', {}, f.until || f.from),
        h('td', {}, [f.map ? (d.maps.find((m) => m.key === f.map) || {}).map || f.map : tr('Every map'), f.kind === 'strike' ? ' · ' + (f.company ? (everyone.find((c) => c.id === f.company) || {}).name || f.company : tr('Every company')) : (f.weather ? ' · ' + f.weather.replace(/^Weather\//, '') : '')].join('')),
        h('td', { class: 'sp-acts' }, sureButton(tr('End'), tr('Sure?'), () => post('events', { do: 'majeure_end', id: f.id })))))) : null,
      h('div', { class: 'sp-row', style: 'flex-wrap:wrap' }, fmKind, fmFrom, fmUntil, fmMap, fmCompany, fmWeather,
        btn(tr('Declare force majeure'), () => post('events', { do: 'majeure', kind: fmKind.value, from: fmFrom.value, until: fmUntil.value, map: fmMap.value, company: fmKind.value === 'strike' ? fmCompany.value : '', weather: fmKind.value === 'strike' ? '' : fmWeather.value }), 'sp-btn small main')),
      h('div', { class: 'sp-dim' }, tr('The company is for a strike, the weather for snow or a storm.')));
    // (NexiconDK, 3 November 2026: the supplements for unsocial hours - K11 -, and the driver of
    // the month - K7)
    const whens = [['evening', 'Weekday evenings'], ['night', 'Nights'], ['saturday', 'Saturdays'], ['sunday', 'Sundays and holidays'], ['snow', 'Snow days']];
    const whenName = (w) => tr((whens.find((x) => x[0] === w.kind) || [w.kind, w.kind])[1]) + (w.from !== undefined ? ` ${String(Math.floor(w.from / 60)).padStart(2, '0')}:${String(w.from % 60).padStart(2, '0')}–${String(Math.floor(w.to / 60)).padStart(2, '0')}:${String(w.to % 60).padStart(2, '0')}` : '');
    const suName = keep('su:name', h('input', { placeholder: tr('Name'), style: 'width:160px' }));
    const suWhen = keep('su:when', h('select', {}, ...whens.map(([v, l]) => h('option', { value: v }, tr(l)))));
    const suFrom = keep('su:from', h('input', { type: 'time', value: '18:00', style: 'width:110px' }));
    const suTo = keep('su:to', h('input', { type: 'time', value: '23:00', style: 'width:110px' }));
    const suPct = keep('su:pct', h('input', { type: 'number', min: '0', step: '1', placeholder: '%', style: 'width:80px' }));
    const suKr = keep('su:kr', h('input', { type: 'number', min: '0', step: '1', placeholder: tr('kr. an hour'), style: 'width:110px' }));
    const supplements = card(tr('Supplements for unsocial hours'),
      h('div', { class: 'sp-dim' }, tr("Added to a player's pay for a trip that leaves in their hours: a share of the pay, kroner an hour, or both. Evenings and nights by the clock (past midnight when the end is earlier).")),
      (d.supplements || []).length ? table([tr('Name'), tr('When'), tr('Share'), tr('kr. an hour'), ''], d.supplements.map((x) => h('tr', {}, h('td', {}, h('b', {}, x.name)), h('td', {}, whenName(x.when)), h('td', {}, `${x.percent} %`), h('td', {}, String(x.per_hour)),
        h('td', { class: 'sp-acts' }, sureButton(tr('Remove'), tr('Sure?'), () => post('events', { do: 'supplement_remove', id: x.id })))))) : null,
      h('div', { class: 'sp-row', style: 'flex-wrap:wrap' }, suName, suWhen, suFrom, suTo, suPct, suKr,
        btn(tr('Add the supplement'), () => post('events', { do: 'supplement_add', name: suName.value, when: suWhen.value, from: suFrom.value, to: suTo.value, percent: suPct.value, per_hour: suKr.value }), 'sp-btn small main')),
      h('div', { class: 'sp-dim' }, tr('From and to count for evenings and nights.')));
    const ranking = card(tr('Driver of the month'),
      (d.ranking || []).length
        ? table([tr('Player'), tr('Live timetable hours'), tr('Score')], d.ranking.map((r) => h('tr', {}, h('td', {}, h('b', {}, r.name)), h('td', {}, r.hours.toFixed(1)), h('td', {}, r.score.toFixed(0)))))
        : h('div', { class: 'sp-dim' }, tr('No live trip has passed this month yet.')));
    const number = (value, width = 90, placeholder = '') => h('input', { type: 'number', min: '0', step: '1', value: value ?? '', placeholder, style: `width:${width}px` });
    const dateIn = (value, min) => h('input', { type: 'date', value: value || '', min: min || '', style: 'width:150px' });
    return [rules, holidays, majeure, supplements, ranking, ...d.maps.map((m) => {
      const open = m.open.map((t) => {
        const at = h('input', { type: 'datetime-local', value: local(t.closes), style: 'width:190px' });
        const cap = number(Math.round(t.cap_hour), 90);
        const kind = [t.package ? tag(tr('Package')) : null, t.renewal && !t.event ? tag(tr('Renewal')) : null, t.event ? tag(t.event, 'warn') : null, t.applied.length ? tag(tr('Applied for')) : null, t.direct ? tag(trf('Direct: %{n}', { n: t.direct }), 'ok') : null];
        return h('tr', {},
          h('td', {}, h('b', {}, t.number || t.line), ...kind, t.caption ? h('div', { class: 'bco-sub' }, t.caption) : null, t.event && t.from ? h('div', { class: 'bco-sub' }, `${t.from} – ${t.until}`) : null,
            t.needs ? h('div', { class: 'bco-sub' }, needsText(t.needs)) : null),
          h('td', { class: 'sp-hide' }, t.opens),
          h('td', {}, t.closes),
          h('td', {}, t.bidders ? trf('%{n} bidders', { n: t.bidders }) : h('span', { class: 'sp-dim' }, tr('no bid yet'))),
          h('td', {}, h('div', { class: 'sp-row' }, cap, btn(tr('Set'), () => post('tender', { map: m.key, id: t.id, do: 'cap', cap: cap.value }))),
            h('div', { class: 'bco-sub' }, trf('estimate %{e} · low under %{l}', { e: kr(t.cost_hour), l: kr(t.low_hour) })),
            t.bus_cap ? h('div', { class: 'bco-sub' }, trf('%{n} contract buses: at most %{cap} a day (estimate %{e})', { n: t.buses, cap: kr(t.bus_cap), e: kr(t.bus_cost) })) : null,
            t.reserve ? h('div', { class: 'bco-sub' }, trf('and %{n} reserve buses', { n: t.reserve })) : null),
          h('td', { class: 'sp-acts' }, h('div', { class: 'sp-row', style: 'justify-content:flex-end' }, at,
            btn(tr('Move the close'), () => post('tender', { map: m.key, id: t.id, do: 'close', at: at.value.replace('T', ' ') })),
            sureButton(tr('Call off'), tr('Sure? Fees are paid back'), () => post('tender', { map: m.key, id: t.id, do: 'cancel' })))),
        );
      });
      const decided = m.decided.map((t) => h('tr', {},
        h('td', {}, h('b', {}, t.number || t.line)),
        h('td', { class: 'sp-hide' }, t.closed),
        h('td', {}, t.winner ? [t.winner, tag(t.company ? tr('Company') : tr('Operator'), t.company ? 'ok' : '')] : h('span', { class: 'sp-dim' }, tr('nobody'))),
        h('td', {}, t.winner ? (t.bus_day ? trf('%{h} + %{b} a bus a day', { h: kr(t.hour), b: kr(t.bus_day) }) : kr(t.hour)) : '-',
          t.bids.length ? h('ol', { class: 'bco-bids' }, t.bids.map((b) => h('li', {}, `${b.name}: ${b.bus_day ? trf('%{h} + %{b} a bus a day', { h: kr(b.hour), b: kr(b.bus_day) }) : kr(b.hour)} · ${b.total} ${tr('points')}`, b.rejected ? tag(tr('rejected'), 'bad') : null))) : null),
      ));
      // (the operators' lines: their last day moved, or ended and out to tender at once)
      const held = m.held.map((x) => {
        const until = dateIn(x.until, d.today);
        return h('tr', {}, h('td', {}, h('b', {}, x.number || x.line)), h('td', {}, x.name), h('td', {}, `${x.from} – ${x.until}`, h('div', { class: 'bco-sub' }, trf('next tender from %{d}', { d: x.next }))),
          h('td', { class: 'sp-hide' }, x.gives_up ? trf('gives it up %{date}', { date: x.gives_up }) : '-'),
          h('td', { class: 'sp-acts' }, h('div', { class: 'sp-row', style: 'justify-content:flex-end' }, until,
            btn(tr('Move the end'), () => post('tender', { map: m.key, do: 'operator_until', line: x.line, until: until.value })),
            sureButton(tr('End and tender'), tr('Sure? The line goes out to tender at once'), () => post('tender', { map: m.key, do: 'operator_end', line: x.line })))));
      });
      // (3 November 2026: the companies' contracts - a contract's last day moved)
      const contracts = (m.contracts || []).map((x) => {
        const until = dateIn(x.until, d.earliest_end);
        // (3 November 2026: an extra bus behind a departure of the line - a concert, say)
        const dTour = h('input', { placeholder: tr('Tour'), style: 'width:60px' });
        const dAt = h('input', { type: 'time', style: 'width:100px' });
        const dFrom = dateIn(d.today, d.today);
        const dUntil = dateIn(d.today, d.today);
        return h('tr', {}, h('td', {}, h('b', {}, x.number || x.line)), h('td', {}, x.name), h('td', {}, `${x.from} – ${x.until}`, h('div', { class: 'bco-sub' }, trf('next tender from %{d}', { d: x.next }))),
          h('td', { class: 'sp-acts' }, h('div', { class: 'sp-row', style: 'justify-content:flex-end' }, until,
            btn(tr('Move the end'), () => post('tender', { map: m.key, do: 'contract_until', company: x.company, line: x.line, until: until.value }))),
            h('div', { class: 'sp-row', style: 'justify-content:flex-end;margin-top:6px' }, dTour, dAt, dFrom, dUntil,
              btn(tr('Order a doubling'), () => post('tender', { map: m.key, do: 'doubling', company: x.company, line: x.line, tour: dTour.value, at: dAt.value, from: dFrom.value, until: dUntil.value }, trf('An extra bus behind the %{t} of tour %{n}: on the duty board from %{d}', { t: dAt.value, n: dTour.value, d: dFrom.value })))),
            changeRow(x)));
      });
      const free = (m.lines || []).filter((l) => !l.company && !l.operator && !l.open && !l.event);
      // (3 November 2026: a contract changed - the map's new timetable, a line added or taken out)
      function changeRow(x) {
        const kind = h('select', {}, h('option', { value: 'timetable' }, tr("The map's new timetable")), h('option', { value: 'add' }, tr('Add a line')), h('option', { value: 'remove' }, tr('Take a line out')));
        const addLine = h('select', {}, ...free.map((l) => h('option', { value: l.name }, l.name)));
        const removeLine = h('select', {}, ...(x.lines || [x.line]).map((l) => h('option', { value: l }, l)));
        const other = () => (kind.value === 'add' ? addLine.value : kind.value === 'remove' ? removeLine.value : '');
        const show = () => {
          addLine.style.display = kind.value === 'add' ? '' : 'none';
          removeLine.style.display = kind.value === 'remove' ? '' : 'none';
        };
        kind.addEventListener('change', show);
        show();
        return h('div', { class: 'sp-row', style: 'justify-content:flex-end;margin-top:6px' }, kind, addLine, removeLine,
          sureButton(tr('Change'), trf('Sure? It applies in %{n} days', { n: d.change_days || 2 }), () => post('events', { do: 'change', company: x.company, line: x.line, kind: kind.value, other: other() })));
      }
      // (3 November 2026: roadworks on a line - the stops between closed, the minutes it adds)
      const rwLine = keep(m.key + ':rw-line', h('select', {}, ...(m.lines || []).map((l) => h('option', { value: l.name }, l.name))));
      const rwA = keep(m.key + ':rw-a', h('input', { placeholder: tr('From stop'), style: 'width:150px' }));
      const rwB = keep(m.key + ':rw-b', h('input', { placeholder: tr('To stop'), style: 'width:150px' }));
      const rwFrom = keep(m.key + ':rw-from', dateIn(d.today, d.today));
      const rwUntil = keep(m.key + ':rw-until', dateIn(d.today, d.today));
      const rwMin = keep(m.key + ':rw-min', number('', 70, tr('min')));
      const roadworks = h('div', { class: 'sp-card', style: 'background:var(--sp-field)' },
        h('h3', {}, tr('Roadworks')),
        h('div', { class: 'sp-dim' }, tr("The stops between the two close (OMSI drives the map's roads: no road round it). A journey of the line is not fined for being late by the minutes, and is paid them on top.")),
        (m.roadworks || []).length ? table([tr('Line'), tr('Stretch'), tr('Days'), ''], m.roadworks.map((r) => h('tr', {}, h('td', {}, h('b', {}, r.number || r.line)),
          h('td', {}, `${r.from_stop} – ${r.to_stop}`, h('div', { class: 'bco-sub' }, r.closed.length ? trf('closed: %{s}', { s: r.closed.join(', ') }) : tr('no stop between'))),
          h('td', {}, `${r.from} – ${r.until}`, h('div', { class: 'bco-sub' }, trf('+%{n} min', { n: r.minutes }))),
          h('td', { class: 'sp-acts' }, sureButton(tr('End'), tr('Sure?'), () => post('events', { do: 'roadworks_end', id: r.id })))))) : null,
        h('div', { class: 'sp-row', style: 'flex-wrap:wrap' }, rwLine, rwA, rwB, rwFrom, rwUntil, rwMin,
          btn(tr('Set the roadworks'), () => post('events', { do: 'roadworks', map: m.key, line: rwLine.value, from_stop: rwA.value, to_stop: rwB.value, from: rwFrom.value, until: rwUntil.value, minutes: rwMin.value }), 'sp-btn small main')));
      // (the map's free lines, several at once; before the market has read them: names typed)
      const pick = keep(m.key + ':lines', free.length ? h('select', { multiple: true, size: Math.min(6, free.length), style: 'min-width:200px' }, ...free.map((l) => h('option', { value: l.name }, l.name))) : h('input', { placeholder: tr("The lines' timetable names, comma between") }));
      const picked = () => (pick.tagName === 'SELECT' ? [...pick.selectedOptions].map((o) => o.value) : pick.value.split(',').map((x) => x.trim()).filter(Boolean));
      const opensIn = keep(m.key + ':opens', h('input', { type: 'datetime-local', style: 'width:190px' }));
      const closesIn = keep(m.key + ':closes', h('input', { type: 'datetime-local', style: 'width:190px' }));
      const daysIn = keep(m.key + ':days', number('', 70, '3'));
      const capIn = keep(m.key + ':cap', number('', 90, tr('kr.')));
      // (3 November 2026: several lines as one package, and its reserve buses' share)
      const packIn = keep(m.key + ':package', h('input', { placeholder: tr('e.g. Ahlheim By'), style: 'width:190px' }));
      const reserveIn = keep(m.key + ':reserve', number('', 70, '10'));
      // (3 November 2026: what it asks of the buses - floor, fuel, age, screens)
      const opt = (v, l, on) => h('option', { value: v, selected: !!on }, tr(l));
      const floorIn = keep(m.key + ':floor', h('select', {}, ...FLOOR_RULES.map(([v, l]) => opt(v, l, v === 'low_or_entry'))));
      const fuelIn = keep(m.key + ':fuel', h('select', {}, ...FUEL_RULES.map(([v, l]) => opt(v, l, v === 'any'))));
      const ageIn = keep(m.key + ':age', number('', 70, '12'));
      const screensIn = keep(m.key + ':screens', h('input', { type: 'checkbox' }));
      const putOut = h('div', { class: 'sp-card', style: 'background:var(--sp-field)' },
        h('h3', {}, tr('Put out to tender')),
        h('div', { class: 'sp-row', style: 'align-items:flex-start;flex-wrap:wrap' }, pick,
          h('div', { style: 'display:flex;flex-direction:column;gap:6px' },
            h('label', { class: 'sp-row' }, h('span', { class: 'sp-dim', style: 'width:150px' }, tr('Opens (empty: now)')), opensIn),
            h('label', { class: 'sp-row' }, h('span', { class: 'sp-dim', style: 'width:150px' }, tr('Closes (or days)')), closesIn, daysIn),
            h('label', { class: 'sp-row' }, h('span', { class: 'sp-dim', style: 'width:150px' }, tr('Cap a timetable hour')), capIn, h('span', { class: 'sp-dim' }, tr('empty: the rules\''))),
            h('label', { class: 'sp-row' }, h('span', { class: 'sp-dim', style: 'width:150px' }, tr('As one package, named')), packIn, h('span', { class: 'sp-dim' }, tr('empty: each line its own tender'))),
            h('label', { class: 'sp-row' }, h('span', { class: 'sp-dim', style: 'width:150px' }, tr('Reserve buses (%)')), reserveIn, h('span', { class: 'sp-dim' }, tr('empty: the rules\''))),
            h('label', { class: 'sp-row' }, h('span', { class: 'sp-dim', style: 'width:150px' }, tr('Floor')), floorIn),
            h('label', { class: 'sp-row' }, h('span', { class: 'sp-dim', style: 'width:150px' }, tr('Fuel')), fuelIn),
            h('label', { class: 'sp-row' }, h('span', { class: 'sp-dim', style: 'width:150px' }, tr('Buses at most (years)')), ageIn, h('span', { class: 'sp-dim' }, tr('empty: 12, 0: any age'))),
            h('label', { class: 'sp-row' }, h('span', { class: 'sp-dim', style: 'width:150px' }, tr('Passenger information')), screensIn, h('span', { class: 'sp-dim' }, tr('screens in every bus (the Buses tab says which have them)'))),
            btn(tr('Open the tender'), () => {
              const lines = picked();
              if (!lines.length) return say(tr('Choose the lines'), true);
              const pkg = packIn.value.trim();
              if (pkg && lines.length < 2) return say(tr('A package has two lines or more'), true);
              post('tender', { map: m.key, do: 'open', lines, package: pkg, reserve: reserveIn.value, opens: opensIn.value.replace('T', ' '), closes: closesIn.value.replace('T', ' '), days: daysIn.value, cap: capIn.value, floor: floorIn.value, fuel: fuelIn.value, max_age: ageIn.value, screens: screensIn.checked }, pkg ? trf('The package %{p} (%{l}) goes out to tender in a moment', { p: pkg, l: lines.join(', ') }) : trf('%{l} go out to tender in a moment', { l: lines.join(', ') }));
            }, 'sp-btn small main'))));
      // (a line awarded directly to a company of the map - no bids, at the admin's price)
      const directLines = (m.lines || []).filter((l) => !l.company && !l.event && !l.operator);
      const dLine = keep(m.key + ':d-line', h('select', {}, ...directLines.map((l) => h('option', { value: l.name }, l.name + (l.open ? ` (${tr('open tender')})` : '')))));
      const dCompany = keep(m.key + ':d-company', h('select', {}, ...(m.companies || []).map((c) => h('option', { value: c.id }, c.name))));
      const dPrice = keep(m.key + ':d-price', number('', 90, tr('kr.')));
      const dDrive = keep(m.key + ':d-drive', h('select', {}, h('option', { value: 'diesel' }, tr('Diesel buses')), h('option', { value: 'cng' }, tr('CNG buses')), h('option', { value: 'electric' }, tr('Electric buses'))));
      const direct = (m.companies || []).length && directLines.length ? h('div', { class: 'sp-card', style: 'background:var(--sp-field)' },
        h('h3', {}, tr('Award directly (emergency service)')),
        h('div', { class: 'sp-dim' }, tr('The line goes to the company without bidding, at your price a timetable hour; its open tender ends so. A line an operator runs: end its term first (below).')),
        h('div', { class: 'sp-row', style: 'flex-wrap:wrap' }, dLine, dCompany, dPrice, h('span', { class: 'sp-dim' }, tr('kr. a timetable hour')), dDrive,
          sureButton(tr('Award'), tr('Click again to award it directly: no bids, no operators'), () => post('tender', { map: m.key, do: 'direct', line: dLine.value, company: dCompany.value, cap: dPrice.value, drive: dDrive.value })))) : null;
      // (3 November 2026: rail replacement - only where the map has a timetable for it, H9)
      const rlLine = keep(m.key + ':rl-line', h('select', {}, ...(m.rail_lines || []).map((l) => h('option', { value: l }, l))));
      const rlFrom = keep(m.key + ':rl-from', dateIn(d.today, d.today));
      const rlUntil = keep(m.key + ':rl-until', dateIn(d.today, d.today));
      const rlPrice = keep(m.key + ':rl-price', number('', 90, tr('kr.')));
      const rlCompany = keep(m.key + ':rl-company', h('select', {}, h('option', { value: '' }, tr('By a tender of a day')), ...(m.companies || []).map((c) => h('option', { value: c.id }, trf('Directly to %{c}', { c: c.name })))));
      const rail = (m.rail_lines || []).length ? h('div', { class: 'sp-card', style: 'background:var(--sp-field)' },
        h('h3', {}, tr('Rail replacement')),
        h('div', { class: 'sp-dim' }, tr("A timetable of the map's for rail replacement, run on your days: bids for a day at your fixed price a timetable hour - the quality decides - or given to a company directly.")),
        (m.rails || []).length ? table([tr('Line'), tr('Days'), tr('A timetable hour'), ''], m.rails.map((r) => h('tr', {}, h('td', {}, h('b', {}, r.line)), h('td', {}, `${r.from} – ${r.until}`),
          h('td', {}, `${r.price} ${tr('kr.')}`), h('td', {}, r.direct ? trf('Directly to %{c}', { c: r.direct }) : r.offered ? tr('out to tender') : tr('at the next step'))))) : null,
        h('div', { class: 'sp-row', style: 'flex-wrap:wrap' }, rlLine, rlFrom, rlUntil, rlPrice, h('span', { class: 'sp-dim' }, tr('kr. a timetable hour')), rlCompany,
          btn(tr('Order rail replacement'), () => post('tender', { map: m.key, do: 'rail', line: rlLine.value, from: rlFrom.value, until: rlUntil.value, price: rlPrice.value, company: rlCompany.value }), 'sp-btn small main'))) : null;
      // (3 November 2026: the losing companies' complaints about awards, with the bids - H11)
      const complaints = (m.complaints || []).map((k) => h('div', { class: 'sp-card', style: 'background:var(--sp-field)' },
        h('h3', {}, trf('Complaint about %{n}', { n: k.number })),
        h('div', {}, h('b', {}, k.name), ' · ', trf('won by %{w}, starts %{d}', { w: k.winner, d: k.start })),
        h('div', { class: 'sp-dim', style: 'white-space:pre-wrap' }, k.reason),
        table([tr('Bidder'), tr('A timetable hour'), tr('Points')], k.bids.map((b) => h('tr', {}, h('td', {}, b.name), h('td', {}, b.rejected ? '–' : `${b.hour} ${tr('kr.')}`), h('td', {}, b.rejected ? tr('rejected') : String(b.total))))),
        h('div', { class: 'sp-row' }, btn(tr('Let the award stand'), () => post('tender', { map: m.key, id: k.id, do: 'complaint', company: k.company, verdict: 'stand' })),
          k.next ? sureButton(trf('Give it to %{n}', { n: k.next }), tr('Sure? The winner gives the contract back'), () => post('tender', { map: m.key, id: k.id, do: 'complaint', company: k.company, verdict: 'uphold' }), 'sp-btn small main') : h('span', { class: 'sp-dim' }, tr('No other valid bid')))));
      const events = (m.events || []).map((e) => h('div', { class: 'bco-alert' }, h('b'), h('div', {}, h('b', {}, e.name), ` · ${e.from} – ${e.until}`,
        e.lines?.length ? h('div', { class: 'bco-sub' }, trf('Its own lines: %{l}', { l: e.lines.join(', ') })) : null,
        e.off?.length ? h('div', { class: 'bco-sub' }, trf('Takes off: %{l}', { l: e.off.join(', ') })) : null)));
      return card(m.map,
        m.drivers && m.drivers.pool ? h('div', { class: 'sp-dim' }, trf('AI drivers on this map: %{h} hired of %{p}', { h: m.drivers.hired, p: m.drivers.pool })) : null,
        h('h3', {}, trf('Open tenders - %{n}', { n: m.open.length })),
        open.length ? table([tr('Line'), [tr('Opens'), 'sp-hide'], tr('Closes'), tr('Bids'), tr('Cap a timetable hour'), ''], open) : h('div', { class: 'sp-dim' }, tr('No tender is open.')),
        h('div', { class: 'sp-dim' }, tr('The bids stay sealed until the close: only how many companies bid shows here.')),
        complaints.length ? h('h3', {}, trf('Complaints - %{n}', { n: complaints.length })) : null,
        ...complaints,
        putOut, direct, rail, roadworks,
        contracts.length ? h('h3', {}, tr("The companies' contracts")) : null,
        contracts.length ? h('div', { class: 'sp-dim' }, trf('A contract moved to end sooner goes out to tender as its end nears: not before %{d}.', { d: d.earliest_end })) : null,
        contracts.length ? table([tr('Line'), tr('Company'), tr('Term'), ''], contracts) : null,
        held.length ? h('h3', {}, tr("The operators' lines")) : null,
        held.length ? table([tr('Line'), tr('Operator'), tr('Term'), [tr('Gives up'), 'sp-hide'], ''], held) : null,
        decided.length ? h('h3', {}, tr('Decided lately')) : null,
        decided.length ? table([tr('Line'), [tr('Closed'), 'sp-hide'], tr('Won by'), tr('A timetable hour')], decided) : null,
        events.length ? h('h3', {}, tr('Events (Chrono)')) : null,
        ...events,
      );
    })];
  }

  // --- 7. the log, and the console ---------------------------------------------------------------

  const logBox = h('div', { class: 'sp-log' });
  logBox.addEventListener('scroll', () => {
    follow = logBox.scrollTop + logBox.clientHeight >= logBox.scrollHeight - 30;
  });
  const logFilter = h('input', { class: 'sp-grow', placeholder: tr('Show only lines with…') });
  const logPick = h('select', { onChange: (e) => setLog(e.target.value) });
  const fillLog = () => {
    const f = logFilter.value.trim().toLowerCase();
    logBox.replaceChildren(...logLines.filter((l) => !f || l.toLowerCase().includes(f)).map((l) => h('div', { class: /warning|WARN/.test(l) ? 'w' : /error|ERROR/.test(l) ? 'e' : '' }, l)));
    if (follow) logBox.scrollTop = logBox.scrollHeight;
  };
  logFilter.addEventListener('input', fillLog);
  function setLog(src) {
    logSrc = src;
    logFrom = 0;
    logLines = [];
    follow = true;
    draw();
    refresh();
  }
  const fillPick = () => {
    const opt = (v, l) => h('option', { value: v, selected: v === logSrc }, l);
    logPick.replaceChildren(opt('main', tr('The main server')),
      ...(data.worlds?.maps || []).filter((m) => m.world).map((m) => opt('world:' + m.folder, trf('World: %{map}', { map: m.folder }))),
      ...(data.companies?.companies || []).filter((c) => !c.group).map((c) => opt('company:' + c.id, trf('Company: %{name}', { name: c.name }))));
  };
  const consoleIn = h('input', { class: 'sp-grow', placeholder: tr('A console command: help lists them'), style: 'font-family:ui-monospace,Consolas,monospace' });
  const consoleOut = h('pre', { class: 'bco-out' });
  const sendLine = async () => {
    const line = consoleIn.value.trim();
    if (!line) return;
    consoleIn.value = '';
    const a = await run(line, ' ');
    consoleOut.textContent = `> ${line}\n${a ?? ''}\n` + consoleOut.textContent;
  };
  consoleIn.addEventListener('keydown', (e) => { if (e.key === 'Enter') sendLine(); });
  const logCard = card(tr('Log'), h('div', { class: 'sp-row' }, logPick, logFilter), logBox);
  const consoleCard = card(tr('Console'), h('div', { class: 'sp-row' }, consoleIn, btn(tr('Run'), sendLine, 'sp-btn main')), consoleOut);
  function logView() {
    fillPick();
    fillLog();
    return [logCard, consoleCard];
  }

  // --- drawing and asking -----------------------------------------------------------------------

  function draw() {
    drawTabs();
    // (5 November 2026: a tab that cannot be drawn says why, instead of keeping what was there)
    let view;
    try {
      view = {
        overview, worlds: () => [worlds()], companies, players, market, buses: busesView, competitors: competitorsView,
        settings: () => [settings], log: logView,
      }[tab]();
    } catch (e) {
      console.error(e);
      view = [card(null, empty(trf('This tab could not be shown: %{why}', { why: (e && e.message) || String(e) })))];
    }
    body.replaceChildren(...view.flat().filter(Boolean));
  }

  /** What the tab shows, asked again (the overview always: the tabs' counts). */
  async function refresh() {
    if (busy) return;
    busy = true;
    try {
      const want = ['overview'];
      if (tab === 'worlds' || tab === 'log' || tab === 'companies') want.push('worlds');
      if (tab === 'companies' || tab === 'log') want.push('companies');
      if (tab === 'players' || tab === 'companies') want.push('players');
      if (tab === 'market') want.push('market');
      if (tab === 'buses') want.push('buses');
      if (tab === 'competitors') want.push('competitors');
      // (5 November 2026: each question on its own - one that fails shows why on its tab, the
      // others still drawn; the overview failing is the server lost)
      const got = await Promise.allSettled(want.map((w) => api.get(w)));
      want.forEach((w, k) => {
        if (got[k].status === 'fulfilled') {
          data[w] = got[k].value;
          delete failed[w];
        } else failed[w] = message(got[k].reason);
      });
      if (got[0].status === 'rejected') throw got[0].reason;
      if (tab === 'companies' && picked.company) companyInfo = await api.get('company?id=' + encodeURIComponent(picked.company)).catch(() => null);
      if (lost) say('');
      lost = false;
    } catch (e) {
      if (!lost) {
        opts.onLost?.(e);
        say(message(e), true);
      }
      lost = true;
    }
    if (tab === 'log') {
      try {
        const q = logSrc === 'main' ? 'log?from=' + logFrom : logSrc.startsWith('world:') ? 'log?world=' + encodeURIComponent(logSrc.slice(6)) : 'log?company=' + encodeURIComponent(logSrc.slice(8));
        const l = await api.get(q);
        if (logSrc === 'main') {
          logLines.push(...(l?.lines || []));
          if (logLines.length > 2000) logLines.splice(0, logLines.length - 2000);
          if (l?.next != null) logFrom = l.next;
        } else logLines = l?.lines || [];
      } catch {}
    }
    busy = false;
    // (a field being typed in, a choice open, the settings: not drawn anew)
    const typing = body.contains(document.activeElement) && /INPUT|SELECT|TEXTAREA/.test(document.activeElement?.tagName || '');
    const armed = body.querySelector('.sp-btn.sure');
    if (tab === 'log') {
      drawTabs();
      if (!typing) fillPick();
      fillLog();
    } else if (tab !== 'settings' && !typing && !armed) draw();
    else drawTabs();
  }

  draw();
  refresh();
  timer = setInterval(refresh, opts.every || 3000);
  return {
    destroy() {
      clearInterval(timer);
      el.remove();
    },
  };
}
