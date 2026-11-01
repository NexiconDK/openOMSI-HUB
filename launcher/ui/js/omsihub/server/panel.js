// Omsi-Hub (NexiconDK, 1 November 2026): a dedicated server's admin panel - one page for two
// places: the server's own web port (`GET /admin`, crates/omsi-net/src/admin_page.html, which the
// server binary carries: `admin_web::PANEL_JS`) and the launcher's Multiplayer → My server
// (`pages/my-server.js`). Plain DOM, no imports: what it needs comes in `api`.
//
//   api.state()            → { status, players, admin, uptime, remote, reached_at, live_map }
//   api.log(from)          → { lines, next }
//   api.command(lines)     → admin commands, one a line (`kick 3`, `clock 30600`, `say …`)
//   api.config()           → { text, path }       api.saveConfig(text)
//   api.logout?()
//
// `opts.tr` translates (the launcher's; the web page's is English), `opts.onLost` is told when
// the server no longer answers or the login ran out, `opts.tabs` picks the tabs shown, `opts.open`
// opens a page of the server's (the launcher's: in the browser), `opts.stop` false hides "Stop".

const CSS = `
.sp{--sp-accent:var(--hub-accent,#e8a030);--sp-text:var(--hub-text,#eceef2);--sp-dim:var(--hub-text-dim,#9aa0aa);--sp-card:var(--hub-panel,#1d2026);--sp-field:var(--hub-field,#262a31);--sp-line:var(--hub-hairline,#30343c);--sp-danger:var(--hub-danger,#e5534b);--sp-ok:#4cb782;--sp-on-accent:var(--hub-on-accent,#1a1205);
  color:var(--sp-text);font:14px/1.45 Inter,system-ui,sans-serif;display:flex;flex-direction:column;gap:12px;min-width:0}
.sp *{box-sizing:border-box}
.sp-tabs{display:flex;gap:4px;flex-wrap:wrap;border-bottom:1px solid var(--sp-line);padding-bottom:6px}
.sp-tab{background:none;border:0;color:var(--sp-dim);padding:7px 12px;border-radius:8px;cursor:pointer;font:inherit;font-weight:600}
.sp-tab.on{color:var(--sp-text);background:var(--sp-field)}
.sp-tab .sp-n{display:inline-block;min-width:18px;margin-left:6px;padding:0 5px;border-radius:9px;background:var(--sp-accent);color:var(--sp-on-accent);font-size:11px;text-align:center}
.sp-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:12px}
.sp-card{background:var(--sp-card);border:1px solid var(--sp-line);border-radius:12px;padding:14px;display:flex;flex-direction:column;gap:10px;min-width:0}
.sp-card h3{margin:0;font-size:12px;letter-spacing:.06em;text-transform:uppercase;color:var(--sp-dim);font-weight:700}
.sp-head{display:flex;align-items:center;gap:12px;flex-wrap:wrap}
.sp-head h2{margin:0;font-size:20px}
.sp-chip{display:inline-flex;align-items:center;gap:6px;padding:3px 10px;border-radius:99px;background:var(--sp-field);font-size:12px;font-weight:600}
.sp-chip i{width:8px;height:8px;border-radius:50%;background:var(--sp-ok)}
.sp-chip.off i{background:var(--sp-danger)}
.sp-facts{display:grid;grid-template-columns:repeat(auto-fit,minmax(120px,1fr));gap:10px}
.sp-fact b{display:block;font-size:18px}
.sp-fact span{color:var(--sp-dim);font-size:12px}
.sp-row{display:flex;gap:8px;align-items:center;flex-wrap:wrap}
.sp-row>.sp-grow{flex:1;min-width:120px}
.sp-btn{background:var(--sp-field);color:var(--sp-text);border:1px solid var(--sp-line);border-radius:8px;padding:6px 11px;font:inherit;font-weight:600;cursor:pointer;white-space:nowrap}
.sp-btn:hover{border-color:var(--sp-accent)}
.sp-btn.main{background:var(--sp-accent);color:var(--sp-on-accent);border-color:transparent}
.sp-btn.danger{color:var(--sp-danger)}
.sp-btn.sure{background:var(--sp-danger);color:#fff;border-color:transparent}
.sp-btn:disabled{opacity:.45;cursor:default}
.sp-btn.small{padding:3px 8px;font-size:12px}
.sp input,.sp select,.sp textarea{background:var(--sp-field);color:var(--sp-text);border:1px solid var(--sp-line);border-radius:8px;padding:6px 9px;font:inherit;min-width:0}
.sp textarea{width:100%;min-height:340px;font:12px/1.45 ui-monospace,Consolas,monospace}
.sp-table{width:100%;border-collapse:collapse;font-size:13px}
.sp-table th{text-align:left;color:var(--sp-dim);font-size:11px;text-transform:uppercase;letter-spacing:.05em;padding:6px 8px;border-bottom:1px solid var(--sp-line)}
.sp-table td{padding:7px 8px;border-bottom:1px solid var(--sp-line);vertical-align:middle}
.sp-table td.sp-acts{white-space:nowrap;text-align:right}
.sp-dim{color:var(--sp-dim)}
.sp-empty{color:var(--sp-dim);padding:18px;text-align:center}
.sp-log{background:#0d0f12;color:#cfd3da;border-radius:10px;padding:10px;font:12px/1.5 ui-monospace,Consolas,monospace;height:420px;overflow:auto;white-space:pre-wrap;word-break:break-word}
.sp-log .w{color:#e8a030}.sp-log .e{color:#ff7b72}
.sp-say{font-size:12px;color:var(--sp-dim);min-height:1em}
.sp-form{display:grid;grid-template-columns:minmax(150px,220px) 1fr;gap:8px 14px;align-items:center}
.sp-form label{color:var(--sp-dim);font-size:13px}
.sp-form .sp-help{grid-column:2;color:var(--sp-dim);font-size:12px;margin-top:-4px}
.sp-check{display:flex;align-items:center;gap:8px}
@media (max-width:640px){.sp-form{grid-template-columns:1fr}.sp-form .sp-help{grid-column:1}.sp-table .sp-hide{display:none}}
`;

function h(tag, attrs, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v == null || v === false) continue;
    if (k.startsWith('on')) el.addEventListener(k.slice(2).toLowerCase(), v);
    else if (k === 'class') el.className = v;
    else if (k === 'value') el.value = v;
    else if (k === 'checked') el.checked = !!v;
    else el.setAttribute(k, v === true ? '' : v);
  }
  for (const k of kids.flat()) {
    if (k == null || k === false) continue;
    el.append(k instanceof Node ? k : document.createTextNode(String(k)));
  }
  return el;
}

function addCss() {
  if (document.getElementById('sp-css')) return;
  document.head.append(h('style', { id: 'sp-css' }, CSS));
}

const pad = (n) => String(n).padStart(2, '0');
const hms = (s) => `${pad(Math.floor(s / 3600) % 24)}:${pad(Math.floor(s / 60) % 60)}`;
const span = (s) => (s >= 86400 ? `${Math.floor(s / 86400)} d ` : '') + `${Math.floor((s % 86400) / 3600)} h ${pad(Math.floor((s % 3600) / 60))} min`;
const busName = (f) => (f || '').split(/[\\/]/).pop().replace(/\.(bus|ovh)$/i, '');
const weatherName = (f) => (f || '').split(/[\\/]/).pop().replace(/\.owt$/i, '');

/** A button that asks once more before it does what it says (`sure`): no browser dialog. */
function sureButton(label, sureLabel, run, cls = 'sp-btn small danger') {
  const b = h('button', { class: cls, type: 'button' }, label);
  let armed = null;
  b.addEventListener('click', () => {
    if (armed) {
      clearTimeout(armed);
      armed = null;
      b.className = cls;
      b.textContent = label;
      run();
      return;
    }
    b.className = 'sp-btn small sure';
    b.textContent = sureLabel;
    armed = setTimeout(() => {
      armed = null;
      b.className = cls;
      b.textContent = label;
    }, 3500);
  });
  return b;
}

// --- server.cfg as a form ---------------------------------------------------------------------

/** The keys the form shows: [key, label, kind, help]. */
export const CFG_FIELDS = [
  ['General'],
  ['name', 'Name', 'text', "Shown in the players' server list"],
  ['motd', 'Message of the day', 'text', 'Shown when a player joins'],
  ['map', 'Map', 'text', 'e.g. maps/Berlin-Spandau/global.cfg'],
  ['max_players', 'Most players', 'number'],
  ['Start'],
  ['date', 'Date', 'text', 'YYYY-MM-DD, empty: today'],
  ['time', 'Time of day', 'text', 'HH:MM'],
  ['weather', 'Weather', 'text', 'A .owt of the OMSI folder; empty: the map\'s; "cycle": through the day'],
  ['World'],
  ['traffic', 'Traffic', 'number', 'Random cars'],
  ['timetable', 'Timetable buses', 'bool'],
  ['passengers', 'Passengers', 'bool'],
  ['radius', 'Tiles kept loaded', 'number', '0: all of them'],
  ['Clock and weather'],
  ['time_speed', 'Clock speed', 'number', '1 = real time, up to 30'],
  ['real_time', "The machine's real date and time", 'bool'],
  ['metar_sync', 'Real weather (METAR)', 'bool'],
  ['metar_station', 'METAR airport', 'text', 'ICAO code, e.g. EDDB; empty: the nearest'],
  ['Network'],
  ['port', 'UDP port', 'number'],
  ['web_port', 'Web port', 'number', 'Status, live map, this panel'],
  ['tunnel', 'Cloudflare tunnel', 'bool', 'An https address for players anywhere'],
  ['share_positions', 'Live map for everyone', 'bool', '/kort and /players on the web port'],
  ['share_timetable', "Timetable to the players' launchers", 'bool'],
  ['web_admin', 'Admin panel from elsewhere', 'bool', 'Off: only on this machine. On: anywhere the web port is reached, with the admin password'],
  ['Administration'],
  ['admin_password', 'Admin password', 'password', 'At least 8 characters; empty: nobody administers the server'],
  ['vehicles', 'Buses allowed', 'text', 'Vehicle files separated by ; - empty: every bus'],
];

/** server.cfg's `key = value` lines (the first of each key, lower case). */
/** What the server takes for a key server.cfg leaves out (`omsi_lan::server_cfg`). */
export const CFG_DEFAULTS = { max_players: '16', time: '08:00', traffic: '30', timetable: '1', passengers: '1', radius: '0', time_speed: '1', real_time: '0', metar_sync: '0', port: '27015', web_port: '27025', tunnel: '1', share_positions: '0', share_timetable: '1', web_admin: '0' };

export function cfgValues(text) {
  const out = {};
  for (const line of (text || '').split(/\r?\n/)) {
    const l = line.trim();
    if (!l || l.startsWith('#')) continue;
    const k = l.indexOf('=');
    if (k < 0) continue;
    const key = l.slice(0, k).trim().toLowerCase();
    if (!(key in out)) out[key] = l.slice(k + 1).trim();
  }
  return out;
}

/** server.cfg with `key` set to `value`: its line changed where it is (comments kept), else
 * added at the end. */
export function cfgSet(text, key, value) {
  const lines = (text || '').split(/\r?\n/);
  const k = lines.findIndex((line) => {
    const l = line.trim();
    return !l.startsWith('#') && l.includes('=') && l.slice(0, l.indexOf('=')).trim().toLowerCase() === key;
  });
  const line = `${key} = ${value}`;
  if (k >= 0) lines[k] = line;
  else {
    while (lines.length && lines[lines.length - 1].trim() === '') lines.pop();
    lines.push(line, '');
  }
  return lines.join('\n');
}

function settingsView(api, tr, say) {
  const box = h('div', { class: 'sp-card' }, h('h3', {}, tr('Server settings')), h('div', { class: 'sp-dim' }, tr('Reading server.cfg…')));
  let text = '';
  let raw = false;
  const inputs = {};
  const draw = () => {
    box.replaceChildren(h('h3', {}, tr('Server settings')));
    if (raw) {
      const area = h('textarea', { spellcheck: 'false', value: text, onInput: (e) => (text = e.target.value) });
      box.append(area);
    } else {
      const v = { ...CFG_DEFAULTS, ...cfgValues(text) };
      const form = h('div', { class: 'sp-form' });
      for (const [key, label, kind, help] of CFG_FIELDS) {
        if (!label) {
          form.append(h('h3', { style: 'grid-column:1/-1;margin-top:8px' }, tr(key)));
          continue;
        }
        const value = v[key] ?? '';
        let input;
        if (kind === 'bool') {
          input = h('input', { type: 'checkbox', checked: ['1', 'true', 'yes', 'on'].includes(value.toLowerCase()) });
          input = h('div', { class: 'sp-check' }, input);
        } else {
          input = h('input', { type: kind === 'password' ? 'password' : kind === 'number' ? 'number' : 'text', value, autocomplete: 'off' });
        }
        inputs[key] = { kind, el: kind === 'bool' ? input.firstChild : input };
        form.append(h('label', {}, tr(label)), input);
        if (help) form.append(h('div', { class: 'sp-help' }, tr(help)));
      }
      box.append(form);
    }
    const note = h('div', { class: 'sp-say' }, tr('The server reads its settings when it starts: restart it for changes to take effect.'));
    box.append(
      note,
      h('div', { class: 'sp-row' },
        h('button', { class: 'sp-btn main', type: 'button', onClick: save }, tr('Save')),
        h('button', { class: 'sp-btn', type: 'button', onClick: () => { collect(); raw = !raw; draw(); } }, tr(raw ? 'As a form' : 'Edit server.cfg as text')),
      ),
    );
  };
  const collect = () => {
    if (raw) return;
    const v = { ...CFG_DEFAULTS, ...cfgValues(text) };
    for (const [key, { kind, el }] of Object.entries(inputs)) {
      const now = kind === 'bool' ? (el.checked ? '1' : '0') : el.value.trim();
      const was = v[key] ?? '';
      const same = kind === 'bool' ? (['1', 'true', 'yes', 'on'].includes(was.toLowerCase()) ? '1' : '0') === now : was === now;
      if (!same) text = cfgSet(text, key, now);
    }
  };
  async function save() {
    collect();
    const pw = cfgValues(text).admin_password || '';
    if (pw && pw.length < 8) {
      say(tr('The admin password needs at least 8 characters.'), true);
      return;
    }
    try {
      await api.saveConfig(text);
      say(tr('Saved. The server reads it at its next start.'));
    } catch (e) {
      say(String(e?.message || e), true);
    }
  }
  api.config().then((c) => {
    text = c?.text || '';
    draw();
  }, (e) => {
    box.replaceChildren(h('h3', {}, tr('Server settings')), h('div', { class: 'sp-dim' }, String(e?.message || e)));
  });
  return box;
}

/** server.cfg as a form alone (the launcher's, while the server is stopped). */
export function mountSettings(root, api, opts = {}) {
  addCss();
  const tr = opts.tr || ((s) => s);
  const said = h('div', { class: 'sp-say' });
  const say = (text, bad) => {
    said.textContent = text;
    said.style.color = bad ? 'var(--sp-danger)' : '';
  };
  const el = h('div', { class: 'sp' }, settingsView(api, tr, say), said);
  root.replaceChildren(el);
  return { destroy() { el.remove(); } };
}

// --- the panel --------------------------------------------------------------------------------

export function mountPanel(root, api, opts = {}) {
  addCss();
  const tr = opts.tr || ((s) => s);
  const trf = (s, vars) => Object.entries(vars).reduce((t, [k, v]) => t.replaceAll(`%{${k}}`, v), tr(s));
  const tabs = opts.tabs || ['overview', 'players', 'log', 'settings'];
  let tab = tabs[0];
  let st = null;
  let lost = false;
  let timer = null;
  let logFrom = 0;
  const logLines = [];
  let follow = true;
  let notice = 0;

  const said = h('div', { class: 'sp-say' });
  const say = (text, bad) => {
    said.textContent = text;
    said.style.color = bad ? 'var(--sp-danger)' : '';
  };
  const run = async (lines, done) => {
    try {
      await api.command(Array.isArray(lines) ? lines : [lines]);
      if (done) say(done);
      setTimeout(refresh, 700);
    } catch (e) {
      say(String(e?.message || e), true);
    }
  };

  const el = h('div', { class: 'sp' });
  root.replaceChildren(el);
  const tabBar = h('div', { class: 'sp-tabs' });
  const body = h('div', { style: 'display:flex;flex-direction:column;gap:12px' });
  el.append(tabBar, body, said);
  const settings = tabs.includes('settings') ? settingsView(api, tr, say) : null;
  const logBox = h('div', { class: 'sp-log' });
  logBox.addEventListener('scroll', () => {
    follow = logBox.scrollTop + logBox.clientHeight >= logBox.scrollHeight - 30;
  });

  const peers = () => {
    const list = st?.admin?.peers || [];
    const shown = st?.players || [];
    // (every player who has joined; what the live map knows of each beside it)
    const out = list.map((p) => ({ ...p, ...(shown.find((x) => x.id === p.id) || {}) }));
    for (const x of shown) if (!out.some((p) => p.id === x.id)) out.push(x);
    return out;
  };

  function drawTabs() {
    const labels = { overview: 'Overview', players: 'Players', log: 'Log', settings: 'Settings' };
    tabBar.replaceChildren(
      ...tabs.map((t) => {
        const n = t === 'players' ? peers().length : 0;
        return h('button', { class: 'sp-tab' + (t === tab ? ' on' : ''), type: 'button', onClick: () => { tab = t; draw(); } }, tr(labels[t]), n ? h('span', { class: 'sp-n' }, n) : null);
      }),
    );
  }

  function overview() {
    const s = st?.status || {};
    const a = st?.admin || {};
    const real = !!a.real_time;
    const head = h('div', { class: 'sp-card' },
      h('div', { class: 'sp-head' },
        h('h2', {}, s.name || tr('Server')),
        h('span', { class: 'sp-chip' + (lost ? ' off' : '') }, h('i'), tr(lost ? 'Not answering' : 'Running')),
        st?.live_map ? h('a', { href: '/kort', target: '_blank', style: 'color:var(--sp-accent);font-weight:600', onClick: opts.open ? (e) => { e.preventDefault(); opts.open('/kort'); } : null }, tr('Live map')) : null,
      ),
      s.motd ? h('div', { class: 'sp-dim' }, s.motd) : null,
      h('div', { class: 'sp-facts' },
        fact(`${s.players ?? 0} / ${s.max_players ?? '?'}`, tr('Players')),
        fact(a.time || s.time || '-', s.date || tr('Time')),
        fact(real ? tr('Real time') : `${a.speed ?? 1}×`, tr('Clock speed')),
        fact(weatherName(a.weather_file || s.weather) || '-', tr('Weather')),
        fact(st ? span(st.uptime || 0) : '-', tr('Running for')),
        fact(String(s.map || '').replace(/^maps[\\/]/i, '').replace(/[\\/]global\.cfg$/i, '') || '-', tr('Map')),
      ),
      st?.reached_at ? h('div', { class: 'sp-dim' }, trf('Players reach it at %{addr}', { addr: st.reached_at })) : null,
    );
    // the clock
    const timeIn = h('input', { type: 'time', value: (a.time || '').slice(0, 5), style: 'width:120px' });
    const speeds = [1, 2, 5, 10, 20, 30];
    const speedSel = h('select', { disabled: real }, ...speeds.map((n) => h('option', { value: n, selected: Math.round(a.speed || 1) === n }, `${n}×`)));
    const clock = h('div', { class: 'sp-card' }, h('h3', {}, tr('Clock')),
      real ? h('div', { class: 'sp-dim' }, tr("The clock follows this machine's real time.")) : null,
      h('div', { class: 'sp-row' },
        h('button', { class: 'sp-btn', type: 'button', disabled: real, onClick: () => run('time -3600', tr('An hour back')) }, '−1 h'),
        h('button', { class: 'sp-btn', type: 'button', disabled: real, onClick: () => run('time 3600', tr('An hour on')) }, '+1 h'),
        timeIn,
        h('button', { class: 'sp-btn', type: 'button', disabled: real, onClick: () => {
          const [hh, mm] = timeIn.value.split(':').map(Number);
          if (Number.isFinite(hh) && Number.isFinite(mm)) run(`clock ${hh * 3600 + mm * 60}`, trf('The clock set to %{time}', { time: timeIn.value }));
        } }, tr('Set')),
      ),
      h('div', { class: 'sp-row' }, h('span', { class: 'sp-dim' }, tr('Speed')), speedSel,
        h('button', { class: 'sp-btn', type: 'button', disabled: real, onClick: () => run(`speed ${speedSel.value}`, trf('The clock runs %{n}× now', { n: speedSel.value })) }, tr('Set'))),
    );
    // the weather and the traffic
    const files = a.weathers || [];
    const wSel = h('select', { class: 'sp-grow' }, ...files.map((f) => h('option', { value: f, selected: f.toLowerCase() === (a.weather_file || '').toLowerCase() }, weatherName(f))));
    const metar = !!a.metar;
    const weather = h('div', { class: 'sp-card' }, h('h3', {}, tr('Weather')),
      metar ? h('div', { class: 'sp-dim' }, tr('The weather follows the real METAR report.')) : null,
      h('div', { class: 'sp-row' },
        h('button', { class: 'sp-btn', type: 'button', disabled: metar, onClick: () => run('weather next', tr('The next weather')) }, tr('Next weather')),
      ),
      files.length ? h('div', { class: 'sp-row' }, wSel, h('button', { class: 'sp-btn', type: 'button', disabled: metar, onClick: () => run(`weather set ${wSel.value}`, trf('The weather now %{w}', { w: weatherName(wSel.value) })) }, tr('Set'))) : null,
    );
    const w = s.world || {};
    const tIn = h('input', { type: 'number', min: 0, max: a.traffic_max || 500, value: a.traffic ?? w.traffic ?? 0, style: 'width:90px' });
    const traffic = h('div', { class: 'sp-card' }, h('h3', {}, tr('Traffic')),
      h('div', { class: 'sp-facts' }, fact(w.cars ?? '-', tr('Cars')), fact(w.buses ?? '-', tr('Buses')), fact(w.waiting ?? '-', tr('Waiting')), fact(w.aboard ?? '-', tr('Aboard'))),
      h('div', { class: 'sp-row' }, h('span', { class: 'sp-dim' }, tr('Density')), tIn,
        h('button', { class: 'sp-btn', type: 'button', onClick: () => run(`traffic ${Math.max(0, Number(tIn.value) || 0)}`, tr('Traffic set')) }, tr('Set')),
        sureButton(tr('Clear the roads'), tr('Sure?'), () => run('traffic clear', tr('Every AI car off the road')), 'sp-btn')),
    );
    // a word to everybody
    const msg = h('input', { class: 'sp-grow', placeholder: tr('A message to every player'), maxlength: 180 });
    const send = (as) => {
      const t = msg.value.trim();
      if (!t) return;
      notice += 1;
      run(as === 'notice' ? `notify all admin-${Date.now()}-${notice} 10 info ${t}` : `say ${t}`, tr('Sent'));
      msg.value = '';
    };
    const message = h('div', { class: 'sp-card' }, h('h3', {}, tr('Message')),
      h('div', { class: 'sp-row' }, msg),
      h('div', { class: 'sp-row' },
        h('button', { class: 'sp-btn main', type: 'button', onClick: () => send('chat') }, tr('In the chat')),
        h('button', { class: 'sp-btn', type: 'button', onClick: () => send('notice') }, tr('On their screens'))),
    );
    const stop = opts.stop !== false ? h('div', { class: 'sp-row' }, sureButton(tr('Stop the server'), tr('Sure? Every player is sent away'), () => run('stop', tr('The server stops')), 'sp-btn danger')) : null;
    return [head, h('div', { class: 'sp-grid' }, clock, weather, traffic, message), stop];
  }

  function fact(value, label) {
    return h('div', { class: 'sp-fact' }, h('b', {}, value), h('span', {}, label));
  }

  function players() {
    const list = peers();
    const card = h('div', { class: 'sp-card' }, h('h3', {}, tr('Players')));
    if (!list.length) {
      card.append(h('div', { class: 'sp-empty' }, tr('Nobody is on the server.')));
      return card;
    }
    const table = h('table', { class: 'sp-table' },
      h('tr', {}, h('th', {}, tr('Player')), h('th', {}, tr('Bus')), h('th', { class: 'sp-hide' }, tr('Line')), h('th', { class: 'sp-hide' }, tr('Speed')), h('th', {}, '')));
    for (const p of list) {
      const what = p.on_foot ? tr('On foot') : busName(p.bus) || (p.driving === false ? tr('Loading') : '-');
      const line = [p.line, p.tour ? `(${p.tour})` : ''].filter(Boolean).join(' ');
      const tell = h('input', { placeholder: tr('A word to him alone'), style: 'width:150px' });
      table.append(
        h('tr', {},
          h('td', {}, h('b', {}, p.name || `#${p.id}`), h('div', { class: 'sp-dim' }, `#${p.id}`)),
          h('td', {}, what, p.passengers ? h('div', { class: 'sp-dim' }, trf('%{n} passengers', { n: p.passengers })) : null),
          h('td', { class: 'sp-hide' }, line || '-'),
          h('td', { class: 'sp-hide' }, p.speed_kmh != null ? `${Math.round(p.speed_kmh)} km/h` : '-'),
          h('td', { class: 'sp-acts' },
            h('div', { class: 'sp-row', style: 'justify-content:flex-end' },
              tell,
              h('button', { class: 'sp-btn small', type: 'button', onClick: () => { const t = tell.value.trim(); if (t) { run(`tell ${p.id} ${t}`, tr('Sent')); tell.value = ''; } } }, tr('Send')),
              h('button', { class: 'sp-btn small', type: 'button', title: tr('Repair'), onClick: () => run(`service repair ${p.id}`, tr('Repaired')) }, tr('Repair')),
              h('button', { class: 'sp-btn small', type: 'button', onClick: () => run(`service refuel ${p.id}`, tr('Refuelled')) }, tr('Refuel')),
              h('button', { class: 'sp-btn small', type: 'button', onClick: () => run(`unstick ${p.id}`, tr('Set free')) }, tr('Unstick')),
              sureButton(tr('Kick'), tr('Sure?'), () => run(`kick ${p.id}`, trf('%{name} sent away', { name: p.name || p.id }))),
              sureButton(tr('Ban'), tr('Sure?'), () => run(`ban ${p.id}`, trf('%{name} sent away for this session', { name: p.name || p.id }))),
            ),
          ),
        ),
      );
    }
    card.append(h('div', { style: 'overflow-x:auto' }, table),
      h('div', { class: 'sp-row' },
        h('button', { class: 'sp-btn', type: 'button', onClick: () => run('service repair all', tr('Every bus repaired')) }, tr('Repair every bus')),
        h('button', { class: 'sp-btn', type: 'button', onClick: () => run('service refuel all', tr('Every bus refuelled')) }, tr('Refuel every bus'))));
    return card;
  }

  // (the log's card made once: its filter keeps what is typed while lines come)
  const logFilter = h('input', { class: 'sp-grow', placeholder: tr('Show only lines with…') });
  const fillLog = () => {
    const f = logFilter.value.trim().toLowerCase();
    logBox.replaceChildren(...logLines.filter((l) => !f || l.toLowerCase().includes(f)).map((l) => h('div', { class: / WARN /.test(l) ? 'w' : / ERROR /.test(l) ? 'e' : '' }, l)));
    if (follow) logBox.scrollTop = logBox.scrollHeight;
  };
  logFilter.addEventListener('input', fillLog);
  const logCard = h('div', { class: 'sp-card' }, h('h3', {}, tr('Log')), h('div', { class: 'sp-row' }, logFilter), logBox);
  function logView() {
    fillLog();
    return logCard;
  }

  function draw() {
    drawTabs();
    if (tab === 'overview') body.replaceChildren(...overview().filter(Boolean));
    else if (tab === 'players') body.replaceChildren(players());
    else if (tab === 'log') body.replaceChildren(logView());
    else if (tab === 'settings' && settings) body.replaceChildren(settings);
  }

  async function refresh() {
    try {
      st = await api.state();
      if (lost) say('');
      lost = false;
    } catch (e) {
      if (!lost) opts.onLost?.(e);
      lost = true;
    }
    // (the settings and a field being typed in are not drawn anew)
    const typing = body.contains(document.activeElement) && /INPUT|SELECT|TEXTAREA/.test(document.activeElement?.tagName || '');
    if (tab !== 'settings' && tab !== 'log' && !typing) draw();
    else drawTabs();
    if (tab === 'log' || logFrom === 0) {
      try {
        const l = await api.log(logFrom);
        if (l?.lines?.length) {
          logLines.push(...l.lines);
          if (logLines.length > 2000) logLines.splice(0, logLines.length - 2000);
          if (tab === 'log') fillLog();
        }
        if (l?.next != null) logFrom = l.next;
      } catch {}
    }
  }

  draw();
  refresh();
  timer = setInterval(refresh, opts.every || 2000);
  return {
    destroy() {
      clearInterval(timer);
      el.remove();
    },
  };
}
