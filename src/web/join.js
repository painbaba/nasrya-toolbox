// The LAN join page: trade a one-time key for a guest cookie, then wait for
// the host to let this browser in. No framework, no build step.
'use strict';

(() => {
  const $ = (id) => document.getElementById(id);
  const steps = ['step-key', 'step-wait', 'step-no'];
  let timer = null;

  function show(step) {
    for (const s of steps) $(s).hidden = s !== step;
    if (step === 'step-key') $('key').focus();
  }

  function stopPolling() {
    if (timer) clearInterval(timer);
    timer = null;
  }

  function refused(title, text) {
    stopPolling();
    $('no-title').textContent = title;
    $('no-text').textContent = text;
    show('step-no');
  }

  /** Act on where this browser stands (from GET or POST /api/join). */
  function handle(st) {
    switch (st.status) {
      case 'in':
      case 'owner':
        stopPolling();
        location.replace('/');
        return;
      case 'waiting':
        show('step-wait');
        if (!timer) timer = setInterval(poll, 1500);
        return;
      case 'denied':
        refused('The host said no', 'That key is used up now. If it was a mistake, ask the host for a new one.');
        return;
      case 'removed':
        refused('The host removed you', 'You no longer have access to this session. Ask the host if you need to get back in.');
        return;
      default:
        stopPolling();
        show('step-key');
    }
  }

  function poll() {
    fetch('/api/join', { cache: 'no-store' })
      .then((r) => r.json())
      .then(handle)
      .catch(() => {}); // the host may have stopped; keep the page as it is
  }

  function error(text) {
    const e = $('join-error');
    e.textContent = text;
    e.hidden = !text;
  }

  // K7F29Q -> K7F-29Q as it's typed; case doesn't matter
  $('key').addEventListener('input', (ev) => {
    const raw = ev.target.value.toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 6);
    ev.target.value = raw.length > 3 ? `${raw.slice(0, 3)}-${raw.slice(3)}` : raw;
    error('');
  });

  $('join-form').addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const key = $('key').value.trim();
    if (key.replace(/-/g, '').length < 6) {
      error('The key has 6 characters, like K7F-29Q.');
      return;
    }
    const btn = $('join-btn');
    btn.disabled = true;
    try {
      const r = await fetch('/api/join', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ key, name: $('name').value.trim() }),
      });
      const body = await r.json().catch(() => ({}));
      if (r.ok) {
        handle(body);
      } else if (body.error === 'wrong') {
        error(body.left > 0
          ? `That key isn't right. ${body.left} ${body.left === 1 ? 'try' : 'tries'} left before it stops working.`
          : 'That key isn\'t right, and it no longer works. Ask the host for a new one.');
      } else if (body.error === 'no_key') {
        error('That key was already used or has expired. Ask the host for a new one.');
      } else {
        error('The host refused the request. Reload the page and try again.');
      }
    } catch {
      error('Can\'t reach the host. Is Winsploit still running in LAN mode?');
    } finally {
      btn.disabled = false;
    }
  });

  $('again').addEventListener('click', () => {
    $('key').value = '';
    error('');
    show('step-key');
  });

  // already asked (a reload while waiting, or after the host decided)?
  poll();
})();
