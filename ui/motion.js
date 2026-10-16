// Shared micro-motion: sliding tab ink, tab-body entrance. Off with data-motion="off" or reduced motion.
(() => {
  const root = document.documentElement;
  const still = () => root.dataset.motion === 'off' || matchMedia('(prefers-reduced-motion: reduce)').matches;
  const EASE = 'cubic-bezier(.2,.9,.25,1)';

  function ink(nav) {
    let i = nav.querySelector(':scope > .tabs-ink');
    if (!i) { i = document.createElement('i'); i.className = 'tabs-ink'; nav.append(i); nav.classList.add('has-ink'); }
    const b = nav.querySelector('button[aria-selected="true"]');
    if (!b || !b.offsetWidth) { i.style.setProperty('opacity', '0'); return; }
    i.style.setProperty('width', `${b.offsetWidth}px`);
    i.style.setProperty('transform', `translateX(${b.offsetLeft}px)`);
    i.style.setProperty('opacity', '1');
    if (!i.dataset.ready) requestAnimationFrame(() => { i.dataset.ready = '1'; });
  }
  const all = () => document.querySelectorAll('nav.tabs').forEach(ink);
  const ro = new ResizeObserver((es) => es.forEach((e) => ink(e.target)));
  document.querySelectorAll('nav.tabs').forEach((n) => { ink(n); ro.observe(n); });

  new MutationObserver((ms) => {
    for (const m of ms) {
      const t = m.target;
      if (m.attributeName === 'aria-selected') { const nav = t.closest('nav.tabs'); if (nav) ink(nav); }
      else if (m.attributeName === 'hidden' && !t.hidden) {
        if (t.classList.contains('tab-body') && !still()) {
          t.animate([{ opacity: 0, transform: 'translateY(6px)' }, { opacity: 1, transform: 'none' }], { duration: 240, easing: EASE });
        }
        // A view or tab that just appeared may hold tabs that were measured at zero width.
        if (t.querySelector?.('nav.tabs')) requestAnimationFrame(all);
      } else if (m.attributeName === 'class' && t.classList?.contains('view') && t.classList.contains('is-active')) {
        requestAnimationFrame(all);
      }
    }
  }).observe(document.body, { subtree: true, attributes: true, attributeFilter: ['aria-selected', 'hidden', 'class'] });
  document.fonts?.ready.then(all);
})();