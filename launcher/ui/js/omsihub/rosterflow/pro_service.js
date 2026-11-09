// RosterFlow Pro's lines and their service (NexiconDK, 7 November 2026, round N: "Har jeg ingen
// mulighed for at Start Service i RosterFlow Pro? Det er der i den normale RosterFlow"). The Vognløb
// tab shows a strip of the company's lines, each with its state today - not in service, in service
// from a moment to come, in service since - and "Start service…" (the Roster tab's dialog: now,
// from tomorrow, from a date) or "Take the line out of service". Every Pro tab says when a line won
// in a tender is not running though its concession has begun (`roster::lines_view`).
//
// Round Q (9 November 2026): the red banner over every Pro tab of the duties without a driver
// signing on soon, and the warning time chosen (`rf_alerts`, `pro_watch`).

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { language } from '../../i18n.js';
import { tr } from '../bits.js';
import { setRf, rfCmd, after, bump } from './state.js';
import { Icon } from './ui.js';
import { ServiceDialog } from './roster.js';
// (round Q: the banner's "Find a driver" opens the Daglig drift tab - read when clicked)
import { PRO_DAILY } from './pro.js';

/** The lines' service (`rf_roster_lines`), asked again when the company or the page changes. */
function useLines() {
  const [v, setV] = useState(null);
  const gen = S.co.shell?.gen ?? 0;
  useEffect(() => {
    let gone = false;
    rfCmd('rf_roster_lines', {})
      .then((x) => !gone && setV(x))
      .catch(() => !gone && setV(null));
    return () => {
      gone = true;
    };
  }, [gen, S.rf.bump, language()]);
  return v;
}

const start = (line) => setRf({ service: { line, when: 0, date: '', gaps: false } });
const stop = async (line) => {
  const r = await rfCmd('rf_roster_act', { act: 'stop', line }).catch((e) => ({ say: { text: e.message, error: true } }));
  after(r);
  bump();
};

/** The banner of the lines won that do not run yet (every Pro tab), and - `full` - the strip of
 * every line with its buttons (the Vognløb tab). */
export function ServiceStrip({ full }) {
  const v = useLines();
  const dialog = S.rf.service && html`<${ServiceDialog} d=${S.rf.service} />`;
  if (!v) return dialog || null;
  const late = v.late?.length > 0 && html`<div class="pro-svc-late">${v.late.map((t) => html`<div><${Icon} name="warning" size=${16} /><span>${t}</span></div>`)}</div>`;
  if (!full) return html`${late}${dialog}`;
  return html`<div class="pro-svc">
    ${late}
    <div class="pro-svc-row">
      <span class="pro-svc-caps">${v.title}</span>
      ${v.lines.map((l) => html`<span class=${'pro-svc-line t-' + l.tone}>
        <b>${l.number}</b><span class="pro-svc-text">${l.text}</span>
        ${l.state === 'on'
          ? html`<button type="button" class="rf-ibtn" title=${v.stop} onClick=${() => stop(l.line)}><${Icon} name="stop_circle" size=${17} /></button>`
          : html`<button type="button" class="rf-btn solid pro-svc-go" onClick=${() => start(l.line)}><${Icon} name="play_arrow" size=${16} /><span>${v.start}</span></button>`}
      </span>`)}
    </div>
    ${dialog}
  </div>`;
}

// --- round Q (NexiconDK, 9 November 2026: "Advarsel om ledige vagter") ---------------------------------

/** How often the duties without a driver are asked for while RosterFlow Pro is open (ms). */
const ALERTS_EVERY = 30000;

/** The duties of the company's day without a driver signing on soon (`rf_alerts`), asked again
 * every half minute, when the company or the page changes. */
function useAlerts() {
  const [v, setV] = useState(null);
  const gen = S.co.shell?.gen ?? 0;
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let gone = false;
    rfCmd('rf_alerts', {})
      .then((x) => !gone && setV(x))
      .catch(() => !gone && setV(null));
    const t = setTimeout(() => setTick((k) => k + 1), ALERTS_EVERY);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  }, [gen, S.rf.bump, language(), tick]);
  return v;
}

/** The duty's box of the Daglig drift tab opened: who can drive it (the round G box). */
function findDriver(date, n) {
  setRf((r) => ({ view: PRO_DAILY, proSel: {}, ops: { ...(r.ops || {}), date, n, pop: true } }));
}

/** (Round Q) The red banner over every Pro tab: the duties of the day without a driver signing on
 * within the warning time - "Find a driver" opens the first one's box; hidden until the list
 * changes when closed. */
export function UnfilledBanner() {
  const v = useAlerts();
  const [hidden, setHidden] = useState('');
  if (!v?.text || !v.items?.length) return null;
  const key = v.items.map((x) => x.n).join(',');
  if (hidden === key) return null;
  const first = v.items[0];
  return html`<div class="pro-unfilled" role="alert">
    <${Icon} name="warning" size=${18} />
    <span class="pro-unfilled-text">${v.text}</span>
    <button type="button" class="rf-btn solid pro-unfilled-go" onClick=${() => findDriver(v.date, first.n)}><${Icon} name="person" size=${16} /><span>${v.open}</span></button>
    <button type="button" class="rf-ibtn" title=${tr('Close')} onClick=${() => setHidden(key)}><${Icon} name="close" size=${16} /></button>
  </div>`;
}

/** (Round Q) The warning time, chosen on the Daglig drift tab's side. */
export function WarnSetting() {
  const v = useAlerts();
  if (!v?.choices) return null;
  const set = async (m) => {
    const r = await rfCmd('rf_ops_act', { act: 'limits', warn: m }).catch((e) => ({ say: { text: e.message, error: true } }));
    after(r);
    bump();
  };
  return html`<div class="pro-caps" style="margin-top:14px">${v.label}</div>
    <select class="pro-select" value=${v.warn} onChange=${(e) => set(Number(e.currentTarget.value))}>
      ${v.choices.map(([m, label]) => html`<option value=${m} selected=${m === v.warn}>${label}</option>`)}
    </select>`;
}
