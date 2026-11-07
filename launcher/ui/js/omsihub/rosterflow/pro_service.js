// RosterFlow Pro's lines and their service (NexiconDK, 7 November 2026, round N: "Har jeg ingen
// mulighed for at Start Service i RosterFlow Pro? Det er der i den normale RosterFlow"). The Vognløb
// tab shows a strip of the company's lines, each with its state today - not in service, in service
// from a moment to come, in service since - and "Start service…" (the Roster tab's dialog: now,
// from tomorrow, from a date) or "Take the line out of service". Every Pro tab says when a line won
// in a tender is not running though its concession has begun (`roster::lines_view`).

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { language } from '../../i18n.js';
import { tr } from '../bits.js';
import { setRf, rfCmd, after, bump } from './state.js';
import { Icon } from './ui.js';
import { ServiceDialog } from './roster.js';

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
