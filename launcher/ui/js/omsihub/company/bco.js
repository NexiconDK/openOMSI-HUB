// Bus Company Online (NexiconDK, phase 2 step 2; the Rust side is `omsihub/bco.rs`, the server's
// `company::server_api`): the third way to play at the bus company's start. The server's code
// once, then the player logged in with his Omsi-Hub account and/or this computer's Steam account,
// the server's companies to join with a click (the company's menu is "Play together"'s:
// `JoinPage`) and a company founded on a map of the server's from his own money there. Every text
// comes from Rust, translated.

import { html, useEffect, useRef, useState, setCo, cmd, Icon, Mark, Btn, Seg, Select, Dialog, Plate, Figure, Section } from './kit.js';
import { run } from './multiplayer.js';
import { toast } from '../../store.js';

/** (Phase 6 step 5) The players' guide to Bus Company Online (`bco_guide`), section by section. */
function GuideDialog({ onClose }) {
  const [g, setG] = useState(null);
  useEffect(() => {
    run('bco_guide').then((r) => r && !r.error && setG(r));
  }, []);
  if (!g) return null;
  return html`<${Dialog} icon="menu_book" title=${g.title} w=${720} h=${640} onClose=${onClose}
    foot=${html`<span class="rest"></span><${Btn} kind="primary" label=${g.close} onClick=${onClose} />`}>
    <div class="bco-guide">${g.sections.map((s) => html`<section><h3>${s.title}</h3><p class="co-para">${s.text}</p></section>`)}</div>
  </${Dialog}>`;
}

/** The server's code and the ways this computer logs in. */
function Connect({ v, again }) {
  const w = v.words;
  const [code, setCode] = useState(v.code || '');
  const [busy, setBusy] = useState(false);
  const [guide, setGuide] = useState(false);
  const go = async () => {
    setBusy(true);
    const r = await run('bco_connect', { code });
    setBusy(false);
    if (r) again(r);
  };
  // (the server's code kept from before: straight in, once)
  useEffect(() => {
    if (v.code && !v.problem) go();
  }, []);
  return html`<div class="co-choose tg-join bco">
    <div style="flex:none"><${Btn} icon="chevron_left" label=${w.back} style="width:180px;height:38px" onClick=${() => setCo({ chooser: 'mode' })} /></div>
    <div class="co-scroll" style="flex:1;margin-top:12px;display:flex;flex-direction:column;align-items:center">
      <span style="flex:0.3 1 0"></span>
      <div class="tg-joincard">
        <span class=${'tg-hero-icon big' + (busy ? ' pulse' : '')}><${Icon} name="public" size=${34} /></span>
        <div class="head"><h1>${w.title}</h1><p>${w.text}</p></div>
        <div class="tg-field"><input class="co-field" value=${code} placeholder=${w.code_field} onInput=${(e) => setCode(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && code.trim() && !busy && go()} />
          <${Btn} kind="primary" icon="link" label=${busy ? w.connecting : w.connect} disabled=${!code.trim() || busy} onClick=${go} /></div>
        ${v.problem && html`<p class="t-warn tg-why">${v.problem}</p>`}
        <ul class="tg-need">${(w.need || []).map((t) => html`<li>${t}</li>`)}</ul>
        ${v.ways?.steam && html`<p class="bco-way"><${Icon} name="sports_esports" size=${16} />${v.ways.steam}</p>`}
        <p class="bco-way"><${Btn} icon="menu_book" label=${w.guide} onClick=${() => setGuide(true)} /></p>
      </div>
      <span style="flex:0.7 1 0"></span>
    </div>
    ${guide && html`<${GuideDialog} onClose=${() => setGuide(false)} />`}
  </div>`;
}

/** A company of the server: its mark, map and mode, its owner, his role in it, Join - a company
 * with depots as the overview of them all (`depots`: how many, folded under it, and the button
 * that unfolds them; no map of its own), each of its departments under it. */
function CompanyRow({ c, w, onJoin, depots = 0, open = false, onFold, onHq }) {
  return html`<div class=${'bco-company' + (c.mine ? ' mine' : '') + (c.depot_of ? ' depot' : '')} data-company=${c.id}>
    <${Mark} mark=${{ colours: c.colours, short: c.short }} size=${48} />
    <div class="who">
      <b class="cut" title=${c.name}>${c.name}</b>
      ${c.depot_of && html`<span class="cut bco-depot-of">${c.depot_of}</span>`}
      <span class="dim cut">${c.map ? `${c.map} · ` : ''}${c.mode} · ${w.owner}: ${c.owner}</span>
      <span class="dim cut">${c.figures}</span>
      <${LinePlates} lines=${c.line_list} />
      ${depots > 0 &&
      html`<button type="button" class=${'bco-fold' + (open ? ' open' : '')} onClick=${onFold}>
        <${Icon} name="garage" size=${15} /><span>${open ? w.hide_depots : w.show_depots}</span><${Icon} name=${open ? 'expand_less' : 'expand_more'} size=${16} />
      </button>`}
    </div>
    ${c.role && html`<span class="bco-role">${c.mine ? w.mine : `${w.your_role}: ${c.role}`}</span>`}
    <span class=${'bco-state' + (c.runs ? ' on' : '')}><i></i>${c.state}</span>
    <span class="bco-acts">
      ${onHq && html`<${Btn} icon="hub" label=${w.hq} onClick=${onHq} />`}
      <${Btn} kind="primary" icon="groups" label=${w.join} disabled=${!c.runs} onClick=${onJoin} />
    </span>
  </div>`;
}

/** How many of a company's line plates its row shows (the rest as "+n"). */
const PLATES = 16;

/** A company's lines as plates in their colours (the map's, as the line editor gave them, or the
 * company's): what a driver looking for a company sees it drive. */
function LinePlates({ lines }) {
  if (!lines?.length) return null;
  const more = lines.length - PLATES;
  return html`<div class="bco-lines">
    ${lines.slice(0, PLATES).map((l) => html`<${Plate} p=${l} h=${22} />`)}
    ${more > 0 && html`<span class="bco-more">+${more}</span>`}
  </div>`;
}

/** (Phase 6) What founding asks besides its price: each need, done or not, and the way to it -
 * the leadership course booked, a driving test booked for a class of bus or called off. */
function Needs({ v, again }) {
  const [cls, setCls] = useState(1);
  const [busy, setBusy] = useState(false);
  const act = async (name, args) => {
    setBusy(true);
    const r = await run(name, args);
    setBusy(false);
    if (r && !r.error) again(r);
  };
  if (!v.needs?.length) return null;
  return html`<div class="bco-needs">
    <label class="bco-label">${v.needs_caps}</label>
    ${v.needs.map((n) => html`<div class=${'bco-need' + (n.ok ? ' ok' : '')} data-need=${n.key}>
      <${Icon} name=${n.ok ? 'check_circle' : 'radio_button_unchecked'} size=${18} />
      <span>${n.text}</span>
      ${n.key === 'test' && n.book && html`<${Select} value=${cls} options=${(n.classes || []).map((c) => c.label)} onChange=${setCls} style="width:160px" />`}
      ${n.book && html`<${Btn} label=${n.book} disabled=${busy} onClick=${() => (n.key === 'test' ? act('bco_test', { class: n.classes?.[cls]?.key || '' }) : act('bco_course'))} />`}
      ${n.cancel && html`<${Btn} label=${n.cancel} disabled=${busy} onClick=${() => act('bco_test', { class: '' })} />`}
    </div>`)}
  </div>`;
}

/** Founding a company on the server: its name, map and game mode, what founding asks, and the
 * price. */
function FoundDialog({ v, onClose, done, again }) {
  const w = v.words;
  const [name, setName] = useState('');
  const [map, setMap] = useState(0);
  const [mode, setMode] = useState('normal');
  const [busy, setBusy] = useState(false);
  const go = async () => {
    setBusy(true);
    const r = await run('bco_found', { name, map: v.maps[map]?.key || '', mode });
    setBusy(false);
    if (r && !r.error) {
      toast(r.message, 'ok');
      done(r.view);
    }
  };
  return html`<${Dialog} icon="add" title=${w.found_title} w=${560} onClose=${onClose}
    foot=${html`<span class="rest"></span><${Btn} label=${w.cancel} onClick=${onClose} /><${Btn} kind="primary" icon="add" label=${w.found_go} disabled=${!name.trim() || busy || !v.me.can_found || !v.maps.length} onClick=${go} />`}>
    <p class="co-para">${w.found_text}</p>
    <label class="bco-label">${w.name}</label>
    <input class="co-field" style="width:100%;height:40px" value=${name} onInput=${(e) => setName(e.currentTarget.value)} />
    <label class="bco-label">${w.map}</label>
    <${Select} value=${map} options=${v.maps.map((m) => m.label)} onChange=${setMap} style="width:100%" />
    <label class="bco-label">${w.mode}</label>
    <${Seg} value=${mode} options=${w.modes.map((m) => [m.key, m.label])} onChange=${setMode} />
    <${Needs} v=${v} again=${again} />
    <p class=${'co-para ' + (v.me.can_found ? 'dim' : 't-warn')} style="margin-top:14px">${v.price}</p>
  </${Dialog}>`;
}

/** The server's companies by head office: each with its depots (a depot whose head office is
 * not on the list stands on its own). A company with depots is an overview of its departments -
 * its lines, buses and depots together, open when one of them is - and its head office one of
 * those departments, named by its map as the depots are ("Umove Ahlheim 5"). */
function grouped(companies, w) {
  const ids = new Set(companies.map((c) => c.id));
  return companies
    .filter((c) => !c.hq || !ids.has(c.hq))
    .map((head) => {
      const depots = companies.filter((c) => c.hq && c.hq === head.id);
      if (!depots.length && !head.group) return { head, overview: head, departments: [] };
      // (phase 7: a group's head office is no department - every one is a depot; a head office of
      // before is one of them, named by its map)
      const office = { ...head, name: `${head.name} ${head.map}`, depot_of: w.head_office };
      const departments = head.group ? depots : [office, ...depots];
      const sum = (k) => departments.reduce((n, d) => n + (d[k] || 0), 0);
      const running = departments.find((d) => d.runs);
      // (its lines: every department's, each once)
      const seen = new Set();
      const lines = departments.flatMap((d) => d.line_list || []).filter((l) => {
        const k = `${l.number}|${l.colour}`;
        if (seen.has(k)) return false;
        seen.add(k);
        return true;
      });
      const overview = {
        ...head,
        map: '',
        line_list: lines,
        figures: w.group_figures.replace('%{lines}', sum('lines')).replace('%{buses}', sum('buses')).replace('%{depots}', departments.length),
        runs: !!running,
        state: (running || head).state,
        group: !!head.group,
        may_run: !!head.may_run,
      };
      return { head, overview, departments };
    });
}

/** The head offices whose depots are unfolded, kept on this computer (a convenience: none when
 * the browser keeps nothing). */
const FOLD_KEY = 'bco-open-depots';
function openDepots() {
  try {
    return new Set(JSON.parse(window.localStorage.getItem(FOLD_KEY) || '[]'));
  } catch {
    return new Set();
  }
}
function keepOpenDepots(set) {
  try {
    window.localStorage.setItem(FOLD_KEY, JSON.stringify([...set]));
  } catch {
    /* (not kept: unfolded for this visit only) */
  }
}

/** Join a company with depots: which department - the head office or one of its depots, each
 * with its map, the player's role in it and whether it is open. */
function WhichDialog({ g, w, onJoin, onClose }) {
  const all = g.departments;
  const [pick, setPick] = useState(Math.max(0, all.findIndex((c) => c.runs)));
  const c = all[pick];
  return html`<${Dialog} icon="groups" title=${w.which_title} w=${560} onClose=${onClose}
    foot=${html`<span class="rest"></span><${Btn} label=${w.cancel} onClick=${onClose} /><${Btn} kind="primary" icon="groups" label=${w.join} disabled=${!c?.runs} onClick=${() => onJoin(c.id)} />`}>
    <p class="co-para">${w.which_text}</p>
    <div class="bco-pick">
      ${all.map((x, k) => html`<button type="button" class=${'bco-pick-row' + (k === pick ? ' on' : '')} disabled=${!x.runs} data-company=${x.id} onClick=${() => setPick(k)}>
        ${k === pick ? html`<${Icon} name="check_circle" size=${18} />` : html`<i class="bco-dot"></i>`}
        <${Mark} mark=${{ colours: x.colours, short: x.short }} size=${32} />
        <span class="who"><b class="cut">${x.name}</b><span class="dim cut">${x.hq ? w.depot : w.head_office} · ${x.map}</span></span>
        ${x.role && html`<span class="bco-role">${x.mine ? w.mine : `${w.your_role}: ${x.role}`}</span>`}
        <span class=${'bco-state' + (x.runs ? ' on' : '')}><i></i>${x.state}</span>
      </button>`)}
    </div>
  </${Dialog}>`;
}

/** Logged in: the server, the player and his money, the companies. */
function Server({ v, again, onHq }) {
  const w = v.words;
  const [founding, setFounding] = useState(false);
  const [guide, setGuide] = useState(false);
  const [busy, setBusy] = useState(false);
  const [open, setOpen] = useState(openDepots);
  const [which, setWhich] = useState(null);
  const fold = (id) => {
    const next = new Set(open);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    keepOpenDepots(next);
    setOpen(next);
  };
  const refresh = async () => {
    setBusy(true);
    const r = await run('bco_refresh');
    setBusy(false);
    if (r) again(r);
  };
  const join = async (id) => {
    const r = await run('bco_join', { company: id });
    if (r?.joined) setCo({ chooser: 'join' });
  };
  const forget = async () => again(await run('bco_forget'));
  return html`<div class="tg-company bco">
    <div class="tg-head">
      <${Btn} icon="chevron_left" label=${w.back} style="width:140px;height:38px" onClick=${() => setCo({ chooser: 'mode' })} />
      <span class="tg-hero-icon"><${Icon} name="public" size=${26} /></span>
      <div class="who"><b class="cut">${v.server.name}</b><span class="cut">${w.server_time}: ${v.server.time} · ${w.players}: ${v.server.players}</span></div>
      <div class="who bco-me"><b class="cut">${v.me.name}</b><span class="cut">${w.money}: ${v.me.money} · ${v.me.by}</span></div>
      <${Btn} icon="menu_book" label=${w.guide} onClick=${() => setGuide(true)} />
      <${Btn} icon="refresh" label=${w.refresh} disabled=${busy} onClick=${refresh} />
      <${Btn} kind="primary" icon="add" label=${w.found} onClick=${() => setFounding(true)} />
      <${Btn} icon="logout" label=${w.forget} onClick=${forget} />
    </div>
    ${v.note && html`<p class="tg-banner t-warn"><${Icon} name="info" size=${16} />${v.note}</p>`}
    ${v.problem && html`<p class="tg-banner t-warn"><${Icon} name="warning" size=${16} />${v.problem}</p>`}
    <div class="co-scroll bco-list">
      <div class="co-caps">${w.companies_caps}</div>
      ${v.companies.length === 0 && html`<p class="dim">${w.none}</p>`}
      ${grouped(v.companies, w).map(
        (g) => html`<${CompanyRow} c=${g.overview} w=${w} depots=${g.departments.length} open=${open.has(g.head.id)} onFold=${() => fold(g.head.id)}
            onJoin=${() => (g.departments.length ? setWhich(g) : join(g.head.id))} onHq=${g.overview.may_run ? () => onHq(g.head.id) : null} />
          ${open.has(g.head.id) && g.departments.map((d) => html`<${CompanyRow} c=${d} w=${w} onJoin=${() => join(d.id)} />`)}`,
      )}
    </div>
    ${founding && html`<${FoundDialog} v=${v} again=${again} onClose=${() => setFounding(false)} done=${(nv) => (setFounding(false), nv && again(nv))} />`}
    ${guide && html`<${GuideDialog} onClose=${() => setGuide(false)} />`}
    ${which && html`<${WhichDialog} g=${which} w=${w} onClose=${() => setWhich(null)} onJoin=${(id) => (setWhich(null), join(id))} />`}
  </div>`;
}

// --- (phase 7) a group's head office: its owner's page (`bco_hq.rs`, the server's `server_hq`) ---

/** A choice among `list` kept by its id (`key`), not its place: the page comes again every 15
 * seconds, a bus moved or a depot founded shifts the rest. [index, set by index]. */
function usePick(list, key = 'id') {
  const [id, setId] = useState(null);
  const k = Math.max(0, list.findIndex((x) => x[key] === id));
  return [k, (i) => setId(list[i]?.[key] ?? null)];
}

/** A field for money, in whole units of the group's currency (empty: none). */
function Money({ value, onChange, placeholder = '', style = 'width:140px' }) {
  return html`<input class="co-field bco-money" type="number" min="0" step="1000" style=${style} placeholder=${placeholder}
    value=${value ?? ''} onInput=${(e) => onChange(e.currentTarget.value)} />`;
}

/** The overview: the depots with their money, month and rule, the requests waiting, the news. */
function HqOverview({ h, act, busy }) {
  const w = h.words;
  return html`<div class="bco-hq-cols">
    <div class="bco-hq-col">
      <${Section} title=${w.depots}>
        <div class="bco-hq-depots">
          ${h.depots.map((d) => html`<div class="bco-hq-depot" data-depot=${d.id}>
            <${Mark} mark=${{ colours: d.colours, short: d.short }} size=${36} />
            <div class="who">
              <b class="cut">${d.name}</b>
              <span class="dim cut"><span class=${'bco-state' + (d.runs ? ' on' : '')}><i></i>${d.map} · ${d.state}</span></span>
              <span class="dim cut">${d.facts}</span>
              <span class="dim cut">${d.rule}</span>
              ${d.red && html`<span class="t-danger cut">${d.red}</span>`}
              ${d.pending && html`<span class="t-ok cut">${d.pending}</span>`}
            </div>
            <div class="bco-hq-money"><b class=${'t-' + d.cash_tone}>${d.cash}</b><span class=${'t-' + d.result_tone}>${d.result}</span></div>
          </div>`)}
        </div>
      </${Section}>
    </div>
    <div class="bco-hq-col">
      ${h.asks.length > 0 &&
      html`<${Section} title=${w.asks}>
        ${h.asks.map((a) => html`<div class="bco-hq-ask"><span>${a.text}</span>
          <${Btn} kind="primary" label=${w.approve} disabled=${busy} onClick=${() => act('answer', { id: a.id, yes: true })} />
          <${Btn} label=${w.refuse} disabled=${busy} onClick=${() => act('answer', { id: a.id, yes: false })} /></div>`)}
      </${Section}>`}
      <${Section} title=${w.news}>
        ${h.inbox.length === 0 && html`<p class="dim">${w.no_news}</p>`}
        <div class="bco-hq-news">
          ${h.inbox.map((m) => html`<div class="bco-hq-item"><span class="dim">${m.date}</span><b class=${'t-' + m.tone}>${m.title}</b><span>${m.text}</span></div>`)}
        </div>
      </${Section}>
    </div>
  </div>`;
}

/** A month of the group's books. */
function HqMonth({ m, w }) {
  return html`<div class="bco-hq-month">
    <div class="co-caps">${m.label}</div>
    <div class="bco-hq-row"><span>${w.income}</span><b class="t-ok">${m.income}</b></div>
    <div class="bco-hq-row"><span>${w.expenses}</span><b class="t-danger">${m.expenses}</b></div>
    <div class="bco-hq-row total"><span>${w.result}</span><b class=${'t-' + m.tone}>${m.result}</b></div>
    ${m.kinds.map((k) => html`<div class="bco-hq-row dim"><span class="cut">${k.label}</span><span class=${'t-' + k.tone}>${k.value}</span></div>`)}
  </div>`;
}

/** A depot's rule: the most it keeps (the rest home at its day's close) and its top-up. */
function HqRule({ d, w, act, busy }) {
  const [keep, setKeep] = useState(d.sweep_over ?? '');
  const [top, setTop] = useState(d.top_up_below ?? '');
  return html`<div class="bco-hq-rule" data-depot=${d.id}>
    <b class="cut">${d.name}</b>
    <label><span>${w.keep}</span><${Money} value=${keep} onChange=${setKeep} /></label>
    <label><span>${w.top_up}</span><${Money} value=${top} onChange=${setTop} /></label>
    <${Btn} label=${w.save} disabled=${busy} onClick=${() => act('rule', { depot: d.id, sweep_over: keep, top_up_below: top })} />
  </div>`;
}

/** Money and books: the group's months, money to a depot, the depots' rules, the loans, a new
 * depot, the treasury's bookings. */
function HqMoney({ h, act, busy }) {
  const w = h.words;
  const [to, setTo] = usePick(h.depots);
  const [amount, setAmount] = useState(h.send_amount);
  const [borrow, setBorrow] = useState(h.borrow_amount);
  const [pay, setPay] = useState({});
  const [map, setMap] = usePick(h.maps, 'key');
  const [start, setStart] = useState(h.start_amount);
  return html`<div class="bco-hq-cols">
    <div class="bco-hq-col">
      <${Section} title=${w.books}>
        <div class="bco-hq-months"><${HqMonth} m=${h.month} w=${w} /><${HqMonth} m=${h.last_month} w=${w} /></div>
      </${Section}>
      <${Section} title=${w.ledger}>
        ${h.ledger.length === 0 && html`<p class="dim">${w.no_ledger}</p>`}
        ${h.ledger.map((b) => html`<div class="bco-hq-row"><span class="dim">${b.date}</span><span class="cut grow">${b.text || b.kind}</span><b class=${'t-' + b.tone}>${b.amount}</b></div>`)}
      </${Section}>
    </div>
    <div class="bco-hq-col">
      ${h.depots.length > 0 &&
      html`<${Section} title=${w.send_caps}>
        <div class="bco-hq-line">
          <${Select} value=${to} options=${h.depots.map((d) => d.name)} onChange=${setTo} style="flex:1;min-width:0" />
          <${Money} value=${amount} onChange=${setAmount} />
          <${Btn} kind="primary" icon="payments" label=${w.send} disabled=${busy || !amount} onClick=${() => act('send', { to: h.depots[to]?.id || '', amount })} />
        </div>
      </${Section}>`}
      ${h.depots.length > 0 &&
      html`<${Section} title=${w.rules}>
        <p class="co-para dim">${w.rules_say}</p>
        ${h.depots.map((d) => html`<${HqRule} key=${d.id} d=${d} w=${w} act=${act} busy=${busy} />`)}
      </${Section}>`}
      <${Section} title=${w.loans}>
        <p class="co-para dim">${h.credit}</p>
        <div class="bco-hq-line"><${Money} value=${borrow} onChange=${setBorrow} /><${Btn} label=${w.borrow} disabled=${busy || !borrow} onClick=${() => act('borrow', { amount: borrow })} /></div>
        ${h.loans.length === 0 && html`<p class="dim">${w.no_loans}</p>`}
        ${h.loans.map((l) => html`<div class="bco-hq-line"><span class="grow">${l.text}</span>
          <${Money} value=${pay[l.id] ?? l.left} onChange=${(x) => setPay({ ...pay, [l.id]: x })} />
          <${Btn} label=${w.repay} disabled=${busy} onClick=${() => act('repay', { loan: l.id, amount: pay[l.id] ?? l.left })} /></div>`)}
      </${Section}>
      <${Section} title=${w.found}>
        <p class="co-para dim">${h.found_say}</p>
        ${h.found_none ? html`<p class="dim">${h.found_none}</p>` : html`<div class="bco-hq-line">
          <label class="grow"><span class="bco-label">${w.map}</span><${Select} value=${map} options=${h.maps.map((m) => m.label)} onChange=${setMap} style="width:100%" /></label>
          <label><span class="bco-label">${w.start}</span><${Money} value=${start} onChange=${setStart} /></label>
          <${Btn} kind="primary" icon="add" label=${w.found} disabled=${busy} onClick=${() => act('found', { map: h.maps[map]?.key || '', start })} />
        </div>`}
      </${Section}>
    </div>
  </div>`;
}

/** One depot's buses and staff: a bus of its own moved to another depot, an employee lent to
 * one - orders the depot does at its next step - what is on its way, the orders waiting. */
function HqDepotFleet({ d, h, act, busy }) {
  const w = h.words;
  const others = h.depots.filter((x) => x.id !== d.id);
  const owned = d.buses.filter((b) => b.owned);
  const free = d.staff.filter((e) => e.free);
  const [bus, setBus] = usePick(owned);
  const [busTo, setBusTo] = usePick(others);
  const [man, setMan] = usePick(free);
  const [manTo, setManTo] = usePick(others);
  const [days, setDays] = useState(7);
  return html`<${Section} title=${d.name}>
    <div class="bco-hq-months">
      <div>
        <div class="co-caps">${w.buses} · ${d.buses.length}</div>
        ${d.buses.length === 0 && html`<p class="dim">${w.no_buses}</p>`}
        <div class="bco-hq-list">${d.buses.map((b) => html`<div class="bco-hq-row"><span class="cut grow">${b.label}</span><span class="dim cut">${b.note || b.kind}</span></div>`)}</div>
        ${owned.length > 0 && others.length > 0 &&
        html`<div class="bco-hq-line">
          <${Select} value=${bus} options=${owned.map((b) => b.label)} onChange=${setBus} style="flex:1;min-width:0" />
          <${Select} value=${busTo} options=${others.map((x) => x.name)} onChange=${setBusTo} style="flex:1;min-width:0" />
          <${Btn} label=${w.move} disabled=${busy} onClick=${() => act('move_bus', { from: d.id, bus: owned[bus]?.id, to: others[busTo]?.id || '' })} />
        </div>`}
      </div>
      <div>
        <div class="co-caps">${w.staff} · ${d.staff.length}</div>
        ${d.staff.length === 0 && html`<p class="dim">${w.no_staff}</p>`}
        <div class="bco-hq-list">${d.staff.map((e) => html`<div class="bco-hq-row"><span class="cut grow">${e.label}</span><span class="dim cut">${e.note || e.role}</span></div>`)}</div>
        ${free.length > 0 && others.length > 0 &&
        html`<div class="bco-hq-line">
          <${Select} value=${man} options=${free.map((e) => `${e.label} (${e.role})`)} onChange=${setMan} style="flex:1;min-width:0" />
          <${Select} value=${manTo} options=${others.map((x) => x.name)} onChange=${setManTo} style="flex:1;min-width:0" />
          <input class="co-field" type="number" min="1" max="90" style="width:64px" value=${days} title=${w.days} onInput=${(e) => setDays(Number(e.currentTarget.value) || 1)} />
          <${Btn} label=${w.lend} disabled=${busy} onClick=${() => act('lend', { from: d.id, employee: free[man]?.id, to: others[manTo]?.id || '', days })} />
        </div>`}
      </div>
    </div>
    ${d.moving.length > 0 && html`<div class="co-caps bco-hq-sub">${w.on_way}</div>${d.moving.map((t) => html`<p class="bco-hq-note">${t}</p>`)}`}
    ${d.orders.length > 0 && html`<div class="co-caps bco-hq-sub">${w.orders}</div>${d.orders.map((t) => html`<p class="bco-hq-note dim">${t}</p>`)}`}
  </${Section}>`;
}

/** Fleet and staff across the depots. */
function HqFleet({ h, act, busy }) {
  return html`<div class="bco-hq-col">
    <p class="co-para dim">${h.move_say} ${h.lend_say}</p>
    ${h.fleet.map((d) => html`<${HqDepotFleet} key=${d.id} d=${d} h=${h} act=${act} busy=${busy} />`)}
  </div>`;
}

/** The group's brand: its name, short name and colours, a depot's mark made the group's, the
 * liveries shared with every depot. */
function HqBrand({ h, act, busy }) {
  const w = h.words;
  const b = h.brand;
  const [name, setName] = useState(b.name);
  const [short, setShort] = useState(b.short);
  const [c1, setC1] = useState(b.colours[0] || '#f28c28');
  const [c2, setC2] = useState(b.colours[1] || '#1d2b44');
  const [from, setFrom] = usePick(h.depots);
  return html`<div class="bco-hq-cols">
    <div class="bco-hq-col">
      <${Section} title=${w.brand_caps}>
        <p class="co-para dim">${w.brand_say}</p>
        <div class="bco-hq-brand">
          <${Mark} mark=${{ colours: [c1, c2], short: short.toUpperCase() }} size=${64} />
          <label class="grow"><span class="bco-label">${w.name}</span><input class="co-field" style="width:100%;height:38px" value=${name} onInput=${(e) => setName(e.currentTarget.value)} /></label>
          <label><span class="bco-label">${w.short}</span><input class="co-field" style="width:90px;height:38px" maxlength="4" value=${short} onInput=${(e) => setShort(e.currentTarget.value)} /></label>
          <label><span class="bco-label">${w.colours}</span><span class="bco-hq-colours">
            <input type="color" value=${c1} onInput=${(e) => setC1(e.currentTarget.value)} /><input type="color" value=${c2} onInput=${(e) => setC2(e.currentTarget.value)} /></span></label>
          <${Btn} kind="primary" label=${w.save} disabled=${busy} onClick=${() => act('brand', { name, short, colours: [c1, c2] })} />
        </div>
        ${b.logo && html`<p class="bco-hq-note dim">${b.logo}</p>`}
        ${b.behind && html`<p class="bco-hq-note t-warn">${b.behind}</p>`}
      </${Section}>
      ${h.depots.length > 0 &&
      html`<${Section} title=${w.mark_of_caps}>
        <p class="co-para dim">${w.mark_of_say}</p>
        <div class="bco-hq-line">
          <${Select} value=${from} options=${h.depots.map((d) => d.name)} onChange=${setFrom} style="flex:1;min-width:0" />
          <${Btn} label=${w.mark_of} disabled=${busy} onClick=${() => act('mark_of', { depot: h.depots[from]?.id || '' })} />
        </div>
      </${Section}>`}
    </div>
    <div class="bco-hq-col">
      <${Section} title=${w.liveries_caps}>
        <p class="co-para dim">${w.liveries_say}</p>
        ${b.liveries.length === 0 && html`<p class="dim">${w.no_liveries}</p>`}
        ${b.liveries.map((l) => html`<div class="bco-hq-row"><b class="cut grow">${l.name}</b><span class="dim cut">${l.bus}</span>
          <${Btn} small label=${w.unshare} disabled=${busy} onClick=${() => act('unshare_livery', { name: l.name })} /></div>`)}
        ${b.depot_liveries.length > 0 && html`<div class="co-caps bco-hq-sub">${w.depot_liveries}</div>`}
        ${b.depot_liveries.map((l) => html`<div class="bco-hq-row"><span class="cut grow">${l.name}</span><span class="dim cut">${l.depot_name} · ${l.bus}</span>
          <${Btn} small label=${w.share} disabled=${busy} onClick=${() => act('share_livery', { depot: l.depot, name: l.name })} /></div>`)}
      </${Section}>
    </div>
  </div>`;
}

/** The group's people: its management and each depot's manager, given by its owner. */
function HqRoles({ h, act, busy }) {
  const w = h.words;
  const [who, setWho] = usePick(h.players);
  const [role, setRole] = useState('management');
  const [depot, setDepot] = usePick(h.depots);
  const p = h.players[who];
  return html`<div class="bco-hq-cols">
    <div class="bco-hq-col">
      <${Section} title=${w.roles_caps}>
        <p class="co-para dim">${w.roles_say}</p>
        <p class="co-para">${h.owner_line}</p>
        ${h.members.length === 0 && html`<p class="dim">${w.no_members}</p>`}
        ${h.members.map((m) => html`<div class="bco-hq-row"><b class="cut grow">${m.name}</b><span class="dim cut">${m.role}</span>
          ${h.is_owner && html`<${Btn} small label=${w.remove} disabled=${busy} onClick=${() => act('role', { player: m.player, name: m.name, role: '' })} />`}</div>`)}
      </${Section}>
    </div>
    <div class="bco-hq-col">
      ${h.is_owner && h.players.length > 0 &&
      html`<${Section} title=${w.give}>
        <label><span class="bco-label">${w.player}</span><${Select} value=${who} options=${h.players.map((x) => x.name)} onChange=${setWho} style="width:100%" /></label>
        <label><span class="bco-label">${w.role}</span><${Seg} value=${role} options=${[['management', w.management], ['depot', w.depot_manager]]} onChange=${setRole} /></label>
        ${role === 'depot' && html`<label><span class="bco-label">${w.depots}</span><${Select} value=${depot} options=${h.depots.map((d) => d.name)} onChange=${setDepot} style="width:100%" /></label>`}
        <div class="bco-hq-line"><span class="grow"></span>
          <${Btn} kind="primary" icon="badge" label=${w.give} disabled=${busy || !p} onClick=${() => act('role', { player: p.id, name: p.name, role, depot: role === 'depot' ? h.depots[depot]?.id || '' : '' })} /></div>
      </${Section}>`}
    </div>
  </div>`;
}

/** The head office's tabs beside its overview. */
const HQ_TABS = { money: HqMoney, fleet: HqFleet, brand: HqBrand, roles: HqRoles };

/** A group's head office (no map: the group's treasury and its depots): its owner's and its
 * management's. */
function HqPage({ group, onBack }) {
  const [h, setH] = useState(null);
  const [tab, setTab] = useState('overview');
  const [busy, setBusy] = useState(false);
  // (shown once: a question that fails later - the server gone a moment - keeps the page)
  const shown = useRef(false);
  const load = async () => {
    const r = await run('bco_hq', { group });
    if (r && !r.error && !r.off) {
      shown.current = true;
      setH(r);
    } else if (r?.error && !shown.current) onBack();
  };
  useEffect(() => {
    load();
    const t = setInterval(load, 15000);
    return () => clearInterval(t);
  }, [group]);
  const act = async (name, args) => {
    setBusy(true);
    const r = await run('bco_hq_do', { group, act: name, args });
    setBusy(false);
    if (r && !r.error) setH(r);
  };
  if (!h) return null;
  const w = h.words;
  return html`<div class="tg-company bco bco-hq" data-group=${h.id}>
    <div class="tg-head">
      <${Btn} icon="chevron_left" label=${w.back} style="width:220px;height:38px" onClick=${onBack} />
      <${Mark} mark=${{ colours: h.colours, short: h.short }} size=${40} />
      <div class="who"><b class="cut">${h.name}</b><span class="cut">${h.under}</span></div>
      <span class="dim bco-hq-time">${h.time}</span>
      <${Btn} icon="refresh" label=${w.refresh} disabled=${busy} onClick=${load} />
    </div>
    <div class="bco-hq-tabs"><${Seg} value=${tab} options=${w.tabs} onChange=${setTab} /></div>
    <div class="co-scroll bco-hq-body">
      <div class="bco-hq-figures">${h.figures.map((f) => html`<${Figure} f=${f} />`)}</div>
      <${HQ_TABS[tab] || HqOverview} h=${h} act=${act} busy=${busy} />
    </div>
  </div>`;
}

/** Bus Company Online: the server's code, then the server. */
export function OnlinePage() {
  const [v, setV] = useState(null);
  const [hq, setHq] = useState(null);
  useEffect(() => {
    cmd('bco_view')
      .then(setV)
      .catch((e) => toast(e.message, 'error'));
  }, []);
  // (the companies' state now and then, while the page is open)
  useEffect(() => {
    if (!v?.linked) return undefined;
    const t = setInterval(() => {
      cmd('bco_refresh')
        .then(setV)
        .catch(() => {});
    }, 15000);
    return () => clearInterval(t);
  }, [v?.linked]);
  if (!v) return null;
  if (v.off) return null;
  const again = (nv) => nv && !nv.error && setV(nv);
  if (!v.linked) return html`<${Connect} v=${v} again=${again} />`;
  if (hq) return html`<${HqPage} group=${hq} onBack=${() => setHq(null)} />`;
  return html`<${Server} v=${v} again=${again} onHq=${setHq} />`;
}
