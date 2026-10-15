'use strict';
// Runs in a sandboxed iframe. `fl.call` is the only way to the launcher.
fl.call('instances.list')
  .then((list) => { document.getElementById('out').textContent = `Сборок: ${list.length}`; fl.resize(); })
  .catch((e) => { document.getElementById('out').textContent = String(e.message); });
