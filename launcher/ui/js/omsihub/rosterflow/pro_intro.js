// RosterFlow Pro's own short intro, "Sådan bruger du RosterFlow Pro" (NexiconDK, 4 November
// 2026, after the player's comments): RosterFlow's tutorial walks its own tabs - Routes,
// Timeplan, Vehicles, Drivers, Roster - which RosterFlow Pro does not have, so the "?" of
// RosterFlow Pro opens this instead: its five tabs in the order they are used. It opens by itself
// the first time RosterFlow Pro is opened on this machine, and when the planner was just chosen.

import { html } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { tr } from '../bits.js';
import { setRf } from './state.js';
import { Icon, RfButton } from './ui.js';
import { PRO_BLOCKS } from './pro.js';

/** The six tabs in their order (`PRO_TABS`' names): icon, number, name, what is done there. */
export const STEPS = [
  ['airport_shuttle', 1, 'Blocks', "The buses' day from the timetables: every bus a row of its trips, a red mark where a driver can change. Read only: the timetables are the lines'."],
  ['badge', 2, 'Duties', 'Cut the blocks at the red marks and put the pieces together as duties - a change of line or bus with its walk between - or let "Make duties automatically" do it. Save.'],
  ['autorenew', 3, 'Rota', 'A rota is several weeks in a row: every week each driver moves a week down, after the last week he begins again at week 1. The drivers work every other weekend; split duties have a rota of their own, Monday to Friday.'],
  ['today', 4, 'Daily operations', 'A day: the duties without a driver and why, who could take each and what he costs. Ask the drivers - they answer as people do - and give the duty to one who says yes.'],
  ['event', 5, 'Calendar', "Every driver's week from Sunday: his sign-on times, driver number, employment and hours this week, and how many duties are missing a driver each day."],
  ['rf_can', 6, 'Can work', 'The hourly paid say a day at a time when they can work - all day, from-to or not that day - and remember it when they are asked. Change a day by clicking it.'],
];

/** The other words of the intro. */
const TITLE = 'How RosterFlow Pro works';
const LEAD = 'Six tabs, used in this order. Hiring and employment are on the Staff page.';

/** The machine's note that the intro was shown once (`localStorage`; none there: shown again). */
const SEEN = 'omsihub.rfpro.intro';

/** Whether RosterFlow Pro is opened the first time on this machine (the intro not seen yet). */
export function proIntroFirst() {
  if (S.boot?.selftest) return false;
  try {
    return !localStorage.getItem(SEEN);
  } catch {
    return false;
  }
}

function seen() {
  try {
    localStorage.setItem(SEEN, '1');
  } catch {
    /* (not kept: shown again next time) */
  }
}

/** The intro opened or closed (RosterFlow Pro's "?"). */
export function proIntroToggle() {
  if (S.rf.proIntro) seen();
  setRf({ proIntro: !S.rf.proIntro });
}

/** Escape: the intro closed first. */
export function proIntroEscape() {
  if (!S.rf.proIntro) return false;
  seen();
  setRf({ proIntro: false });
  return true;
}

export function ProIntro() {
  if (!S.rf.proIntro) return null;
  const close = () => {
    seen();
    setRf({ proIntro: false });
  };
  return html`<div class="rf-sheetdim" onMouseDown=${(e) => e.target === e.currentTarget && close()}>
    <div class="rf-help pro-intro">
      <div class="head"><span class="pro-intro-mark"><${Icon} name="badge" size=${20} /></span><div><b>${tr(TITLE)}</b><span>${tr(LEAD)}</span></div>
        <button type="button" title=${tr('Close')} onClick=${close}><${Icon} name="close" size=${20} /></button></div>
      <div class="pro-intro-steps">
        ${STEPS.map(([icon, n, name, text]) => html`<div class="chain"><span class="badge"><${Icon} name=${icon} size=${15} /></span><div><b>${n} ${tr(name)}</b><p>${tr(text)}</p></div></div>`)}
      </div>
      <div class="pro-intro-foot"><${RfButton} label="Begin with Blocks" solid style="height:32px" onClick=${() => (close(), setRf({ view: PRO_BLOCKS }))} /></div>
    </div>
  </div>`;
}

/** Every English text of the intro (for the translations' check). */
export const TEXTS = [TITLE, LEAD, 'Begin with Blocks', 'Close', ...STEPS.flatMap((s) => [s[2], s[3]])];
