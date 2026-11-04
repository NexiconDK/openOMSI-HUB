// RosterFlow Pro's tabs "4 Daglig drift" and "5 Kalender" (step 4, NexiconDK 4 November 2026; the
// Rust side's `omsihub/rosterflow/ops.rs` and the core's `company::pro_ops`):
//
// * Kalender: every driver a row - his number ("chf."), Fast / Ramme / Timeløn and his week or
//   frame -, each day of the week from Sunday his duty and when he signs on (or a day off, ill, on
//   holiday, a frame), and the hours of his week. Above them each day's duties without a driver; a
//   day clicked opens its daily operations. A week, 14 days or a month.
// * Daglig drift: a date's duties without a driver and why, the reserves' frames, who is free;
//   for the duty chosen every driver in the order it is filled in, whether he is free and what he
//   costs (overtime orange). The drivers ticked are asked; their answers come in - yes, no, too
//   early, too late, a swap - and the duty is given to one who said yes.
//
// Saved at once (`rf_ops_act`), not in RosterFlow's draft.

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { language } from '../../i18n.js';
import { tr } from '../bits.js';
import { setRf, rfCmd, after, bump, say } from './state.js';
import { Icon, RfButton } from './ui.js';
import { PRO_DAILY, PRO_DUTIES } from './pro.js';


/** What the tabs keep: the calendar's week, span and filter; the daily operations' date, the duty
 * chosen and the drivers ticked. */
const st = () => S.rf.ops || {};
const setO = (patch) => setRf((r) => ({ ops: { ...(r.ops || {}), ...(typeof patch === 'function' ? patch(r.ops || {}) : patch) } }));

/** A command's answer, asked again when what it shows or the company changed (and every two
 * seconds while answers are on their way: `again`). */
function useView(cmd, args, keys) {
  const gen = S.co.shell?.gen ?? 0;
  const [v, setV] = useState(null);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let gone = false;
    rfCmd(cmd, args)
      .then((x) => !gone && setV(x))
      .catch((e) => !gone && setV({ reading: e.message }));
    return () => {
      gone = true;
    };
  }, [...keys, gen, S.rf.bump, language(), tick]);
  return [v, () => setTick((t) => t + 1)];
}

/** An action of the daily operations (`rf_ops_act`). */
async function oAct(act, args = {}) {
  let r;
  try {
    r = await rfCmd('rf_ops_act', { act, ...args });
  } catch (e) {
    say(e.message, true);
    return null;
  }
  after(r);
  bump();
  return r;
}

/** The chip of an employment: Fast, Ramme, Timeløn (or a booked agency driver). */
const Kind = ({ kind, chip }) => html`<span class=${'op-kind ' + kind}>${chip}</span>`;

// --- 5 Kalender ------------------------------------------------------------------------------------

const SPANS = ['week', 'two', 'month'];

export function CalendarTab() {
  const o = st();
  const [v] = useView('rf_cal', { week: o.week || 0, span: o.span || 'week', kind: o.kind || 'all' }, [o.week, o.span, o.kind]);
  if (!v) return html`<div class="rf-reading">${tr('Reading the timetable…')}</div>`;
  if (v.reading || v.off) return html`<div class="rf-reading">${v.reading}</div>`;
  const openDay = (date) => setRf((r) => ({ view: PRO_DAILY, ops: { ...(r.ops || {}), date, n: null, ticked: [] } }));
  const weeks = v.drivers[0]?.hours?.length || 1;
  const cols = `150px 96px repeat(${v.days.length}, minmax(${v.days.length > 7 ? 62 : 78}px, 1fr)) ${weeks > 1 ? 64 * Math.min(weeks, 5) : 84}px`;
  return html`<div class="pro-page op-page">
    <div class="rf-toolbar pro-tools">
      <span class="tu-week">
        <button type="button" class="rf-ibtn" disabled=${!(o.week > 0)} title=${tr('The week before')} onClick=${() => setO({ week: Math.max(0, (o.week || 0) - 1) })}><${Icon} name="chevron_left" size=${17} /></button>
        <${RfButton} label="This week" disabled=${!(o.week > 0)} onClick=${() => setO({ week: 0 })} />
        <button type="button" class="rf-ibtn" title=${tr('The week after')} onClick=${() => setO({ week: (o.week || 0) + 1 })}><${Icon} name="chevron_right" size=${17} /></button>
      </span>
      <i class="pro-sep"></i>
      <span class="op-spans">${['Week', '14 days', 'Month'].map((l, k) => html`<button type="button" class=${'rf-link' + ((o.span || 'week') === SPANS[k] ? ' on' : '')} onClick=${() => setO({ span: SPANS[k] })}>${tr(l)}</button>`)}</span>
      <i class="pro-sep"></i>
      <button type="button" class="rf-link" onClick=${() => openDay(null)}><${Icon} name="today" size=${17} /><span>${tr('Daily operations today')}</span></button>
      <span class="grow"></span>
      <span class="tu-week"><b>${v.week.label}</b><span>${v.week.from} – ${v.week.to}</span></span>
    </div>
    <div class="pro-main">
      <aside class="pro-side">
        <div class="pro-caps">${tr('Show')}</div>
        <div class="op-filter">${v.kinds.map(([k, label]) => html`<button type="button" class=${'tu-person-chip' + ((o.kind || 'all') === k ? ' on' : '')} onClick=${() => setO({ kind: k })}><b>${label}</b></button>`)}</div>
        <div class="pro-dim" style="margin-top:6px">${tr('The bus company\'s week begins on Sunday: the hours of the week count from Sunday to Saturday.')}</div>
        <div class="pro-caps" style="margin-top:14px">${tr('Employment')}</div>
        <div class="op-legend">
          <div><${Kind} kind="full" chip=${tr('Permanent')} /><span>${tr('full and part time')}</span></div>
          <div><${Kind} kind="reserve" chip=${tr('Reserve frame')} /><span>${tr('a reserve with set hours')}</span></div>
          <div><${Kind} kind="hourly" chip=${tr('Hourly')} /><span>${tr('no guarantee')}</span></div>
        </div>
        <div class="pro-caps" style="margin-top:14px">${tr('A day')}</div>
        <div class="pro-dim">${tr('A day clicked opens its daily operations: the duties without a driver, and who could take them.')}</div>
      </aside>
      <div class="pro-content">
        <div class="pro-caps head">${v.week.label} · ${v.week.from} – ${v.week.to} · ${tr('the hours count from Sunday')}</div>
        <div class="pro-table op-cal">
          <div class="op-grid op-head" style=${'grid-template-columns:' + cols}>
            <span>${tr('Driver')}</span><span>${tr('Employment')}</span>
            ${v.days.map((d) => html`<button type="button" class=${'op-day' + (d.today ? ' today' : '') + (d.past ? ' past' : '')} title=${d.label} onClick=${() => openDay(d.date)}>${d.wd}<br />${d.day}</button>`)}
            <span class="op-num">${weeks > 1 ? tr('Hours a week') : tr('Hours this week')}</span>
          </div>
          <div class="op-grid op-missing" style=${'grid-template-columns:' + cols}>
            <span class="pro-dim">${tr('Duties without a driver')}</span><span></span>
            ${v.days.map((d) => html`<button type="button" class="op-miss" title=${tr('Open the daily operations of %{day}', { day: d.label })} onClick=${() => openDay(d.date)}>${d.missing ? html`<span class="op-badge bad">${tr('%{n} missing', { n: d.missing })}</span>` : html`<span class="t-ok"><${Icon} name="check" size=${15} /></span>`}</button>`)}
            <span></span>
          </div>
          ${v.drivers.length === 0 && html`<div class="pro-dim tu-none">${tr('No drivers of this kind.')}</div>`}
          ${v.drivers.map((x) => html`<div class="op-grid op-row" style=${'grid-template-columns:' + cols}>
            <span class="op-who"><b>${x.name}</b><span>chf. ${x.nr}</span></span>
            <span class="op-who op-emp"><${Kind} kind=${x.kind} chip=${x.chip} /><span>${x.sub}</span></span>
            ${x.cells.map((c, k) => html`<button type="button" class=${'tu-cell ' + (c.cls || 'off') + (v.days[k].past ? ' op-past' : '')} title=${c.tip || ''} onClick=${() => openDay(v.days[k].date)}>
              <b>${c.label}</b>${c.sub && html`<span>${c.sub}</span>`}${c.frame && html`<i class="op-r" title=${tr('In his frame')}>R</i>`}
            </button>`)}
            <span class="op-hours">${x.hours.map((h) => html`<span title=${h.label} class=${h.over ? 't-warn' : ''}><b>${tr('%{h} h', { h: h.h })}</b>${h.of ? html`<i>${tr('of %{h}', { h: h.of })}</i>` : null}</span>`)}</span>
          </div>`)}
        </div>
      </div>
    </div>
  </div>`;
}

// --- 4 Daglig drift --------------------------------------------------------------------------------

export function DailyTab({ onCalendar }) {
  const o = st();
  const [v, again] = useView('rf_ops', { date: o.date || null, n: o.n ?? null }, [o.date, o.n]);
  // (answers on their way: asked again in two seconds)
  useEffect(() => {
    if (!v?.duty?.waiting) return;
    const t = setTimeout(again, 2000);
    return () => clearTimeout(t);
  }, [v]);
  if (!v) return html`<div class="rf-reading">${tr('Reading the timetable…')}</div>`;
  if (v.reading || v.off) return html`<div class="rf-reading">${v.reading}</div>`;
  const go = (date) => setO({ date, n: null, ticked: [] });
  return html`<div class="pro-page op-page">
    <div class="rf-toolbar pro-tools">
      <button type="button" class="rf-link" onClick=${onCalendar}><${Icon} name="chevron_left" size=${17} /><span>${tr('Back to the calendar')}</span></button>
      <i class="pro-sep"></i>
      <span class="tu-week">
        <button type="button" class="rf-ibtn" disabled=${!v.prev} title=${tr('The day before')} onClick=${() => go(v.prev)}><${Icon} name="chevron_left" size=${17} /></button>
        <${RfButton} label="Today" disabled=${v.today} onClick=${() => go(null)} />
        <button type="button" class="rf-ibtn" title=${tr('The day after')} onClick=${() => go(v.next)}><${Icon} name="chevron_right" size=${17} /></button>
      </span>
      <span class="grow"></span>
      <span class="tu-week"><b>${v.label}</b><span>${tr('%{open} of %{all} duties without a driver', { open: v.count?.open ?? 0, all: v.count?.duties ?? 0 })}</span></span>
    </div>
    <div class="pro-main">
      <aside class="pro-side">
        <div class="pro-caps">${tr('Missing on %{day}', { day: v.label })}</div>
        ${v.open.length === 0 && html`<div class="t-ok op-allok"><${Icon} name="check" size=${15} /> ${tr('Every duty has a driver.')}</div>`}
        ${v.open.map((d) => html`<button type="button" class=${'op-open' + (v.sel === d.n ? ' on' : '')} onClick=${() => setO({ n: d.n, ticked: [] })}>
          <span><b>${d.n}</b> · ${d.time}</span>
          <span class="pro-dim">${tr('line %{l}', { l: d.lines.join(', ') })} · ${d.sign_on}</span>
          <span class="op-why">${d.why}${d.part ? ` · ${tr('partly')}` : ''}${d.asked ? ` · ${tr('%{n} asked', { n: d.asked })}` : ''}</span>
        </button>`)}
        ${v.frames.length > 0 && html`<div class="pro-caps" style="margin-top:14px">${tr('Frames this day')}</div>
          ${v.frames.map((f) => html`<div class="op-line"><${Kind} kind="reserve" chip=${tr('Reserve frame')} /><span><b>${f.name}</b> ${f.time}</span><span class="pro-dim">${f.used ? f.what : tr('free')}</span></div>`)}`}
        ${v.free.length > 0 && html`<div class="pro-caps" style="margin-top:14px">${tr('Free all day')}</div>
          ${v.free.map((f) => html`<div class="op-line"><${Kind} kind=${f.kind} chip=${f.chip} /><span><b>${f.name}</b> chf. ${f.nr}</span></div>`)}`}
      </aside>
      <div class="pro-content">
        ${v.duty ? html`<${Duty} v=${v} d=${v.duty} />` : html`<div class="pro-card">${tr('Nothing to fill on this day: every duty has a driver.')}</div>`}
      </div>
      <aside class="pro-side op-right">
        <div class="pro-caps">${tr('The answers a driver can give')}</div>
        <div class="op-answers">
          ${v.answers.map((a) => html`<div><span class=${'op-badge ' + a.key}>${a.label}</span>${a.key === 'early' ? html`<span class="pro-dim">· ${tr('can from …')}</span>` : a.key === 'late' ? html`<span class="pro-dim">· ${tr('can until …')}</span>` : null}</div>`)}
        </div>
        <div class="pro-caps" style="margin-top:14px">${tr('Reserve frame')}</div>
        <div class="pro-dim">${tr('A reserve given a duty from 12:00 the day before is told at once.')}</div>
        <div class="pro-caps" style="margin-top:14px">${tr('What it costs')}</div>
        <div class="pro-dim">${tr('A reserve within his frame and a permanent driver within his week cost nothing more; above his week it is overtime. The hourly paid are paid at least 3 hours a day called in.')}</div>
      </aside>
    </div>
  </div>`;
}

/** The duty chosen: who could take it and what he costs, the drivers asked and their answers. */
function Duty({ v, d }) {
  const o = st();
  const ticked = o.ticked || [];
  const tickedG = o.tickedG || [];
  const tick = (id) => setO({ ticked: ticked.includes(id) ? ticked.filter((x) => x !== id) : [...ticked, id] });
  const tickG = (no) => setO({ tickedG: tickedG.includes(no) ? tickedG.filter((x) => x !== no) : [...tickedG, no] });
  const ask = () => oAct('ask', { date: v.date, n: d.n, drivers: ticked, guests: tickedG }).then((r) => r?.edited && setO({ ticked: [], tickedG: [] }));
  const give = (id, swap) => oAct('give', { date: v.date, n: d.n, driver: id, swap: swap || null });
  const priceOf = (id) => d.candidates.find((c) => c.id === id);
  return html`
    <div class="pro-card op-card">
      <div class="pro-caps">${tr('Ask for duty %{n}', { n: d.n })} · ${v.label} · ${tr('sign on %{time}', { time: d.sign_on })} · ${d.time} · ${tr('line %{l}', { l: d.lines.join(', ') })} · ${d.kind}</div>
      <div class="op-dim-line">${d.why} · ${tr('paid %{t}', { t: d.paid })}${d.given ? ` · ${tr('given to %{name}', { name: d.given })}` : ''}</div>
      <div class="op-cands">
        <div class="op-cand op-chead"><span></span><span>${tr('Driver')}</span><span>${tr('Employment')}</span><span>${tr('Fill order')}</span><span>${tr('Available')}</span><span>${tr('Price')}</span></div>
        ${d.candidates.map((c) => html`<label class=${'op-cand' + (c.free ? '' : ' no')}>
          <span>${c.askable && html`<input type="checkbox" disabled=${!c.free} checked=${ticked.includes(c.id)} onChange=${() => tick(c.id)} />`}</span>
          <span class="op-who"><b>${c.name}</b><span>chf. ${c.nr}${c.asked ? ` · ${tr('asked for it')}` : ''}</span></span>
          <span><${Kind} kind=${c.kind} chip=${c.chip} /></span>
          <span class="pro-dim">${c.free ? c.tier : '–'}</span>
          <span class="pro-dim">${c.free ? `${c.standby ? tr('free all day') : tr('free then')} · ${tr('%{h} h this week', { h: c.week })}${c.note ? ` · ${c.note}` : ''}` : c.why}</span>
          <span class=${c.over ? 'op-over' : ''}>${c.free ? c.price : ''}</span>
        </label>`)}
        ${d.players.map((p) => html`<label class="op-cand">
          <span><input type="checkbox" checked=${tickedG.includes(p.no)} onChange=${() => tickG(p.no)} /></span>
          <span class="op-who"><b>${p.name}</b><span>${p.asked ? tr('asked for it') : tr('a player')}</span></span>
          <span><span class="op-kind player">${tr('Player')}</span></span>
          <span class="pro-dim">${tr('Colleague')}</span>
          <span class="pro-dim">${tr('answers himself')}</span><span></span>
        </label>`)}
      </div>
      <div class="op-bar">
        <${RfButton} label=${ticked.length + tickedG.length ? tr('Ask %{n}', { n: ticked.length + tickedG.length }) : tr('Ask them')} raw solid disabled=${!(ticked.length + tickedG.length)} onClick=${ask} />
        <span class="pro-dim">${d.agency}</span>
        <span class="grow"></span>
        ${d.given && html`<${RfButton} label="Take it back" onClick=${() => oAct('withdraw', { date: v.date, n: d.n })} />`}
      </div>
    </div>
    <div class="pro-card op-card">
      <div class="pro-caps">${tr('Answers')}</div>
      ${d.asks.length === 0 && html`<div class="pro-dim">${tr('Nobody asked yet: tick the drivers above and ask them.')}</div>`}
      ${d.asks.map((a) => {
        const c = a.nr != null ? priceOf(a.nr) : null;
        const r = a.reply;
        return html`<div class="op-answer">
          <span class="op-who"><b>${a.name}</b><span>${r ? r.words : html`<i class="pro-dim">${tr('thinking about it…')}</i>`}</span></span>
          <span class="op-act">
            ${r && html`<span class=${'op-badge ' + r.key}>${r.chip}</span>`}
            ${r?.yes && a.nr != null && !r.swap && html`<${RfButton} solid raw label=${tr('Give him the duty') + (c ? ` · ${c.price}` : '')} onClick=${() => give(a.nr)} />`}
            ${(r?.yes || r?.key === 'text') && a.who?.kind === 'guest' && html`<${RfButton} solid raw label=${tr('Give him the duty')} onClick=${() => oAct('give_guest', { date: v.date, n: d.n, guest: a.who.no })} />`}
            ${r?.swap && a.nr != null && html`<${RfButton} solid raw label=${tr('Give him the duty, with the swap')} onClick=${() => give(a.nr, r.swap)} />`}
            ${(r?.key === 'early' || r?.key === 'late') && html`<button type="button" class="rf-link" title=${tr('Cut the duty at a relief point on the Vagter tab, and ask again for each piece')} onClick=${() => setRf({ view: PRO_DUTIES })}>${tr('Split the duty')}</button>`}
          </span>
        </div>`;
      })}
    </div>`;
}
