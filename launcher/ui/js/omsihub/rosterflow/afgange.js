// RosterFlow Pro's tab "3 Afgange" (round K, NexiconDK 7 November 2026; the Rust side's
// `omsihub/rosterflow/pro.rs` `departures_json`, `build_preview` and the core's
// `pro_duties::build_duty`): every departure of the day in order of time - its line (a night line
// with its moon), its stops, its bus and the duty it is in - and a duty built one at a time from
// them: the departures clicked into "Duty being built" (the trips between two of the same bus and
// its runs empty to and from them go with them), the duty as it would be - its times, break, type,
// rules and price - shown at once, its own name, and "Save duty" cuts the buses before and after at
// whatever departure (a stop that is no relief point is a yellow mark on the duty). Every edit is
// RosterFlow's: in the draft, with Undo, until SAVE.

import { html, useEffect, useMemo, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { tr } from '../bits.js';
import { setRf, rfAct, rfCmd, edit } from './state.js';
import { Icon, RfButton } from './ui.js';
import { clock, hm, DutyCard } from './pro.js';

/** What the tab keeps while it is open: the departures chosen (`"i:j"`), the name, the filters. */
const st = () => S.rf.dep || {};
const setDep = (patch) => setRf((r) => ({ dep: { ...(r.dep || {}), ...patch } }));
const keyOf = (d) => `${d.i}:${d.j}`;

/** The longest a bus stands between a run without passengers and the duty's trips for the run to
 * go with them (`pro_duties::RUN_GAP`). */
const RUN_GAP = 30;

/** The trips a duty of the chosen departures takes on each bus (`pro_duties::run_of`): from the
 * first to the last chosen, and the runs without passengers just before and after them that the
 * bus goes on with. */
function included(deps, chosen) {
  const byBlock = new Map();
  for (const k of chosen) {
    const [i, j] = k.split(':').map(Number);
    if (!byBlock.has(i)) byBlock.set(i, []);
    byBlock.get(i).push(j);
  }
  const trips = new Map();
  for (const d of deps) {
    if (!trips.has(d.i)) trips.set(d.i, []);
    trips.get(d.i)[d.j] = d;
  }
  const out = new Set();
  for (const [i, js] of byBlock) {
    const t = trips.get(i) || [];
    let a = Math.min(...js);
    let b = Math.max(...js);
    while (a > 0 && t[a - 1] && t[a] && !t[a - 1].counts && t[a].dep - t[a - 1].arr <= RUN_GAP) a -= 1;
    while (t[b + 1] && t[b] && !t[b + 1].counts && t[b + 1].dep - t[b].arr <= RUN_GAP) b += 1;
    for (let j = a; j <= b; j += 1) out.add(`${i}:${j}`);
  }
  return out;
}

/** The runs of the chosen departures, one a bus: its block, times and stops. */
function runsOf(deps, inc) {
  const by = new Map();
  for (const d of deps) {
    if (!inc.has(keyOf(d))) continue;
    const r = by.get(d.i) || { block: d.block, colour: d.colour, own: d.own, from: d.dep, to: d.arr, a: d.from, b: d.to, taken: new Set() };
    if (d.dep < r.from) Object.assign(r, { from: d.dep, a: d.from });
    if (d.arr >= r.to) Object.assign(r, { to: d.arr, b: d.to });
    if (d.duty) r.taken.add(d.duty);
    by.set(d.i, r);
  }
  return [...by.values()].sort((x, y) => x.from - y.from);
}

export function AfgangeTab() {
  const data = S.rf.data;
  if (!data?.board) return html`<div class="rf-reading">${data?.reading || tr('Reading the timetable…')}</div>`;
  const pro = data.board.pro;
  if (!pro) return html`<div class="rf-reading">${tr('RosterFlow Pro is not the planner of this company: choose it on the planning page.')}</div>`;
  return html`<${Departures} data=${data} pro=${pro} />`;
}

function Departures({ data, pro }) {
  const s = st();
  const deps = pro.departures || [];
  const chosen = s.chosen || [];
  const show = s.show || 'all';
  const line = s.line || '';
  const q = (s.q || '').trim().toLowerCase();
  const empty = !!s.empty;
  const inc = useMemo(() => included(deps, chosen), [deps, chosen.join(',')]);
  const lines = useMemo(() => [...new Set(deps.filter((d) => d.counts).map((d) => d.line))].sort((a, b) => a.localeCompare(b, undefined, { numeric: true })), [deps]);
  const rows = deps.filter((d) => {
    if (!d.counts && !empty && !inc.has(keyOf(d))) return false;
    if (show === 'free' && d.duty && !inc.has(keyOf(d))) return false;
    if (line && d.line !== line && d.own !== line) return false;
    if (q && ![d.line, d.from, d.to, d.block, d.duty || ''].some((x) => String(x).toLowerCase().includes(q))) return false;
    return true;
  });
  const free = deps.filter((d) => d.counts && !d.duty).length;
  const toggle = (d) => {
    const k = keyOf(d);
    const next = chosen.includes(k) ? chosen.filter((x) => x !== k) : [...chosen, k];
    setDep({ chosen: next });
  };
  const dirty = !!data.head?.draft;
  return html`<div class="pro-page dep-page">
    <div class="rf-toolbar pro-tools">
      <span class="pair">
        <${RfButton} label="Save" solid disabled=${!dirty} title="Make the changes the company's planning: the roster, the day and the game run it" style="width:88px" onClick=${() => rfAct('save')} />
        <${RfButton} label="Discard" disabled=${!dirty} style="width:96px" onClick=${() => rfAct('discard').then(() => setDep({ chosen: [] }))} />
      </span>
      <i class="pro-sep"></i>
      <span class="dep-seg">
        <button type="button" class=${show === 'all' ? 'on' : ''} onClick=${() => setDep({ show: 'all' })}>${tr('All')}</button>
        <button type="button" class=${show === 'free' ? 'on' : ''} onClick=${() => setDep({ show: 'free' })}>${tr('Without a duty')} · ${free}</button>
      </span>
      <select class="dep-sel" value=${line} onChange=${(e) => setDep({ line: e.target.value })}>
        <option value="">${tr('Every line')}</option>
        ${lines.map((l) => html`<option value=${l}>${tr('Line %{n}', { n: l })}</option>`)}
      </select>
      <label class="dep-search"><${Icon} name="search" size=${15} /><input type="search" value=${s.q || ''} placeholder=${tr('Stop, line, bus or duty')} onInput=${(e) => setDep({ q: e.target.value })} /></label>
      <label class="dep-check"><input type="checkbox" checked=${empty} onChange=${(e) => setDep({ empty: e.target.checked })} />${tr('Runs without passengers')}</label>
      <span class="grow"></span>
      <span class="pro-day">${tr('Day')}: <b>${pro.day_type}</b></span>
    </div>
    <div class="pro-main">
      <div class="pro-content dep-content">
        <div class="pro-caps head">${tr('Departures')} · ${pro.day_type} · ${tr('%{n} departures', { n: deps.filter((d) => d.counts).length })} · ${tr('click the departures of a duty, then save it')}</div>
        <div class="pro-table dep-table">
          <div class="pro-thead"><span class="c-x"></span><span class="c-t">${tr('Time')}</span><span class="c-l">${tr('Line')}</span><span class="c-w">${tr('From → To')}</span><span class="c-t">${tr('Arrives')}</span><span class="c-b">${tr('Block')}</span><span class="c-d">${tr('Duty')}</span></div>
          <div class="pro-rows">
            ${rows.map((d) => {
              const k = keyOf(d);
              const on = chosen.includes(k);
              const with_ = !on && inc.has(k);
              return html`<div class=${'pro-row dep-row' + (on ? ' on' : '') + (with_ ? ' with' : '') + (d.counts ? '' : ' dead')} onClick=${() => toggle(d)} title=${d.relief ? '' : tr('Drivers cannot change at %{stop}: a duty begun here is marked', { stop: d.from })}>
                <span class="c-x"><i class=${'dep-box' + (on ? ' on' : with_ ? ' with' : '')}>${on ? '✓' : with_ ? '·' : ''}</i></span>
                <span class="c-t"><b>${clock(d.dep)}</b></span>
                <span class="c-l"><b class="pro-chip" style=${`--c:${d.colour}`}>${d.line}</b>${d.night ? html`<span class="dep-moon" title=${tr('Night line')}>🌙</span>` : ''}</span>
                <span class="c-w cut">${d.counts ? '' : html`<em class="dep-empty">${tr('empty')}</em> `}${d.from} → ${d.to}${d.relief ? '' : html` <i class="dep-norelief" title=${tr('Not a relief point')}>◇</i>`}</span>
                <span class="c-t">${clock(d.arr)}</span>
                <span class="c-b">${d.block}</span>
                <span class=${'c-d' + (d.duty ? '' : ' none')}>${d.duty || '—'}</span>
              </div>`;
            })}
            ${rows.length === 0 && html`<div class="pro-dim" style="padding:12px">${tr('No departure fits the filters.')}</div>`}
          </div>
        </div>
      </div>
      <aside class="dep-side"><${Building} deps=${deps} chosen=${chosen} inc=${inc} name=${s.name || ''} /></aside>
    </div>
  </div>`;
}

/** "Duty being built": the departures chosen bus by bus, its name, the duty they would make (asked
 * of the Rust side, nothing kept) and "Save duty". */
function Building({ deps, chosen, inc, name }) {
  const [view, setView] = useState({ busy: false, duty: null, error: null });
  const runs = runsOf(deps, inc);
  const trips = chosen.map((k) => k.split(':').map(Number));
  const ask = JSON.stringify([trips, name, S.rf.rev]);
  useEffect(() => {
    if (!trips.length) {
      setView({ busy: false, duty: null, error: null });
      return undefined;
    }
    let live = true;
    setView((v) => ({ ...v, busy: true }));
    const t = setTimeout(async () => {
      const r = await rfCmd('rf_act', { act: 'pro_build_preview', day: S.rf.day, rev: S.rf.rev || '', trips, name }).catch((e) => ({ error: e.message }));
      if (live) setView({ busy: false, duty: r?.duty || null, error: r?.error || r?.say?.text || null });
    }, 160);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [ask]);
  const save = async () => {
    const r = await edit('pro_build', { trips, name });
    if (r?.edited) setDep({ chosen: [], name: '' });
  };
  const taken = [...new Set(runs.flatMap((r) => [...r.taken]))];
  return html`<div class="dep-build">
    <div class="pro-caps">${tr('Duty being built')}</div>
    ${!chosen.length && html`<div class="pro-dim">${tr('Click the departures of the duty in the list: one at a time, of one bus or several. The trips between two of the same bus and its runs to and from them go with them.')}</div>`}
    <label class="dep-name"><span>${tr('Name')}</span><input type="text" maxlength="12" value=${name} placeholder=${tr('its number')} onInput=${(e) => setDep({ name: e.target.value })} /></label>
    ${runs.map((r) => html`<div class="dep-run" style=${`--c:${r.colour}`}><b class="pro-chip" style=${`--c:${r.colour}`}>${r.own}</b><span class="cut"><b>${r.block}</b> · ${clock(r.from)}–${clock(r.to)}</span><em class="cut">${r.a} → ${r.b}</em></div>`)}
    ${taken.length > 0 && html`<div class="pro-dim">${tr('Taken out of %{d}: what is left of them waits in the pool.', { d: taken.join(', ') })}</div>`}
    ${view.error && html`<div class="dep-err">${view.error}</div>`}
    <div class="dep-btns">
      <${RfButton} label="Save duty" solid disabled=${!chosen.length || !!view.error || view.busy} title="Cut the buses before and after these departures and make them one duty" onClick=${save} />
      <${RfButton} label="Clear" disabled=${!chosen.length && !name} onClick=${() => setDep({ chosen: [], name: '' })} />
    </div>
    ${view.duty && html`<div class=${'dep-preview' + (view.busy ? ' busy' : '')}>
      <div class="dep-figs">
        <span><small>${tr('Signs on')}</small><b>${clock(view.duty.start)}</b></span>
        <span><small>${tr('Signs off')}</small><b>${clock(view.duty.end)}</b></span>
        <span><small>${tr('Paid')}</small><b>${hm(view.duty.paid)}</b></span>
        <span><small>${tr('Type')}</small><b class=${'k-' + view.duty.kind}>${view.duty.kind_label}</b></span>
        <span><small>${tr('Break')}</small><b>${view.duty.pause || '–'}</b></span>
      </div>
      <${DutyCard} duty=${view.duty} />
    </div>`}
  </div>`;
}
