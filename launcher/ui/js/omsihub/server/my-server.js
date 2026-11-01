// Omsi-Hub (NexiconDK, 1 November 2026): Multiplayer → My server - a dedicated server on this
// machine run from the launcher instead of a command window (`omsihub/my_server.rs`): its folder,
// start and stop, and the admin panel the server shows on its web port (`panel.js`, the same page
// as `http://<machine>:<web_port>/admin`), asked through the launcher with the server's password.
// Stopped, its settings (server.cfg) are edited here.
//
// (The same day) Or Bus Company Online's main server: its own folder, its panel (`bco-panel.js`,
// the same page as `http://<machine>:<api_port>/admin`), its company-server.cfg.

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { toast } from '../../store.js';
import { t } from '../../i18n.js';
import { call } from '../../api.js';
import { Button, Card, Empty, Notice, Segmented, Spinner, Tag } from '../../ui.js';
import { mountPanel, mountSettings } from './panel.js';
import { mountPanel as mountBco, BCO_FORM } from './bco-panel.js';

const api = {
  state: () => call('srv_api', { path: 'state' }),
  log: (from) => call('srv_api', { path: 'log?from=' + from }),
  command: (lines) => call('srv_api', { path: 'command', body: lines.join('\n') }),
  config: () => call('srv_cfg'),
  saveConfig: (text) => call('srv_cfg_save', { text }),
};

/** The main server's panel's questions (`company::server_admin`): a GET without a body. */
const bcoApi = {
  get: (path) => call('srv_api', { path }),
  post: (path, body) => call('srv_api', { path, body: body ?? '' }),
  config: () => call('srv_cfg'),
  saveConfig: (text) => call('srv_cfg_save', { text }),
};

const KINDS = [['game', 'Multiplayer server'], ['bco', 'Bus Company Online server']];

const message = (e) => String(e?.message || e);

export function MyServer() {
  const [st, setSt] = useState(null);
  const [busy, setBusy] = useState('');
  const host = useRef(null);
  const load = () => call('srv_status').then(setSt).catch((e) => toast(message(e), 'error'));
  useEffect(() => {
    load();
    const id = setInterval(load, 3000);
    return () => clearInterval(id);
  }, []);
  const running = !!st?.running;
  const bco = st?.kind === 'bco';
  // (the panel while it runs, the settings while it does not)
  useEffect(() => {
    if (!host.current || !st?.folder) return undefined;
    const open = (path) => call('open_url', { url: /^https?:/.test(path) ? path : `http://127.0.0.1:${st.web_port}${path}` }).catch(() => {});
    let m;
    if (bco) m = running ? mountBco(host.current, bcoApi, { tr: t, stop: false, onLost: load, open, host: '127.0.0.1' }) : mountSettings(host.current, bcoApi, { tr: t, form: BCO_FORM });
    else m = running ? mountPanel(host.current, api, { tr: t, stop: false, onLost: load, open }) : mountSettings(host.current, api, { tr: t });
    return () => m.destroy();
  }, [running, st?.folder, st?.kind]);

  const choose = async () => {
    const title = bco ? "Choose the main server's folder (the one with its company-server.cfg, or an empty one)" : "Choose the server's folder (the one with its server.cfg, or an empty one)";
    const f = await call('pick_folder', { title: t(title) }).catch(() => null);
    if (f) setSt(await call('srv_folder', { folder: f }).catch((e) => (toast(message(e), 'error'), st)));
  };
  const switchKind = async (kind) => {
    if (kind === st?.kind) return;
    setSt(await call('srv_kind', { kind }).catch((e) => (toast(message(e), 'error'), st)));
  };
  const run = async (what) => {
    setBusy(what);
    try {
      const r = await call(what === 'start' ? 'srv_start' : 'srv_stop');
      if (what === 'start') toast(t(r?.password_made ? 'The server starts. It had no admin password: the launcher gave it one (Settings).' : 'The server starts: its panel opens here as soon as it answers.'), 'ok');
      else toast(t(r?.killed ? 'The server did not stop by itself: it was ended.' : 'The server stopped.'), 'ok');
    } catch (e) {
      toast(message(e), 'error');
    }
    setBusy('');
    load();
  };

  if (!st) return html`<${Card}><${Spinner} /><//>`;
  const kinds = html`<div style="margin-bottom:12px"><${Segmented} value=${st.kind || 'game'} options=${KINDS} onChange=${switchKind} /></div>`;
  if (!st.folder) {
    return html`${kinds}
      <${Card} title=${bco ? 'Bus Company Online server' : 'My server'}>
      ${bco
        ? html`<${Empty} icon="folder" title="Bus Company Online's main server on this machine" text="The companies, the players, the maps' worlds and their tenders - run and administered from here: choose its folder, one with its company-server.cfg or an empty one for a new server." >
            <div style="margin-top:12px"><${Button} kind="primary" icon="folder" label="Choose the main server's folder…" onClick=${choose} /></div>
          <//>`
        : html`<${Empty} icon="folder" title="A dedicated server on this machine" text="Run a server that is always on from here, without a command window: choose its folder - one with its server.cfg, or an empty one for a new server." >
            <div style="margin-top:12px"><${Button} kind="primary" icon="folder" label="Choose the server's folder…" onClick=${choose} /></div>
          <//>`}
    <//>`;
  }
  return html`${kinds}
    <${Card} title=${st.name || (bco ? 'Bus Company Online server' : 'My server')} hint=${st.folder}
      actions=${html`
        <${Tag} kind=${running ? 'ok' : ''}>${t(running ? 'Running' : 'Stopped')}</${Tag}>
        ${running
          ? html`<${Button} kind="danger" icon="stop" label="Stop" disabled=${!!busy} onClick=${() => run('stop')} />`
          : html`<${Button} kind="primary" icon="play" label="Start" disabled=${!!busy} onClick=${() => run('start')} />`}
        ${busy && html`<${Spinner} />`}
        <${Button} icon="external" label="Open in the browser" disabled=${!running} onClick=${() => call('open_url', { url: st.admin_url }).catch(() => {})} />
        <${Button} icon="folder" label="Another folder…" onClick=${choose} />`}>
      ${bco
        ? html`<div class="row" style="gap:16px;flex-wrap:wrap"><span>${t('Door port')}: <b>${st.web_port}</b></span></div>`
        : html`<div class="row" style="gap:16px;flex-wrap:wrap">
            <span>${t('Map')}: <b>${String(st.map || '').replace(/^maps[\\/]/i, '').replace(/[\\/]global\.cfg$/i, '') || '-'}</b></span>
            <span>${t('UDP port')}: <b>${st.port}</b></span>
            <span>${t('Web port')}: <b>${st.web_port}</b></span>
          </div>`}
      ${!st.web_admin && html`<${Notice}>${t(bco
        ? 'The admin panel opens on this machine only. "Admin panel from elsewhere" in the settings opens it from your phone or another PC too - through the tunnel as well - with the admin password.'
        : 'The admin panel opens on this machine only. "Admin panel from elsewhere" in the settings opens it from your phone or another PC too, with the admin password.')}<//>`}
      ${!running && html`<${Notice}>${t('The server is stopped: change its settings below, then start it.')}<//>`}
    <//>
    <div ref=${host} style="margin-top:12px"></div>
  `;
}
