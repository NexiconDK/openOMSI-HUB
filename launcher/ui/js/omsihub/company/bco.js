// Bus Company Online (NexiconDK, phase 2 step 2; the Rust side is `omsihub/bco.rs`, the server's
// `company::server_api`): the third way to play at the bus company's start. The server's code
// once, then the player logged in with his Omsi-Hub account and/or this computer's Steam account,
// the server's companies to join with a click (the company's menu is "Play together"'s:
// `JoinPage`) and a company founded on a map of the server's from his own money there. Every text
// comes from Rust, translated.

import { html, useEffect, useState, setCo, cmd, Icon, Mark, Btn, Seg, Select, Dialog, Plate } from './kit.js';
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
function CompanyRow({ c, w, onJoin, depots = 0, open = false, onFold }) {
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
    <${Btn} kind="primary" icon="groups" label=${w.join} disabled=${!c.runs} onClick=${onJoin} />
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
      if (!depots.length) return { head, overview: head, departments: [] };
      const office = { ...head, name: `${head.name} ${head.map}`, depot_of: w.head_office };
      const departments = [office, ...depots];
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
function Server({ v, again }) {
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
            onJoin=${() => (g.departments.length ? setWhich(g) : join(g.head.id))} />
          ${open.has(g.head.id) && g.departments.map((d) => html`<${CompanyRow} c=${d} w=${w} onJoin=${() => join(d.id)} />`)}`,
      )}
    </div>
    ${founding && html`<${FoundDialog} v=${v} again=${again} onClose=${() => setFounding(false)} done=${(nv) => (setFounding(false), nv && again(nv))} />`}
    ${guide && html`<${GuideDialog} onClose=${() => setGuide(false)} />`}
    ${which && html`<${WhichDialog} g=${which} w=${w} onClose=${() => setWhich(null)} onJoin=${(id) => (setWhich(null), join(id))} />`}
  </div>`;
}

/** Bus Company Online: the server's code, then the server. */
export function OnlinePage() {
  const [v, setV] = useState(null);
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
  return html`<${Server} v=${v} again=${again} />`;
}
