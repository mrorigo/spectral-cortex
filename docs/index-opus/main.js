/* ============================================================
   Spectral Cortex — Landing Page Scripts
   ============================================================ */

(function () {
  'use strict';

  /* --- Intersection Observer: fade-in on scroll --- */
  const observerOptions = {
    root: null,
    rootMargin: '0px 0px -60px 0px',
    threshold: 0.12,
  };

  const animateIn = (entries, observer) => {
    entries.forEach((entry) => {
      if (entry.isIntersecting) {
        entry.target.classList.add('is-visible');
        observer.unobserve(entry.target);
      }
    });
  };

  const io = new IntersectionObserver(animateIn, observerOptions);

  // Observe all major content blocks
  const targets = document.querySelectorAll(
    '.pain-card, .feat-card, .step, .tool-item, .uc-card, .spec-card, ' +
    '.section-label, .section-title, .section-desc, .terminal, ' +
    '.hero-badge, .hero h1, .hero-sub, .hero-actions, .hero-install, ' +
    '.cta h2, .cta .hero-actions, .cta .hero-install'
  );

  targets.forEach((el, i) => {
    el.style.opacity = '0';
    el.style.transform = 'translateY(24px)';
    el.style.transition = `opacity 0.6s cubic-bezier(0.4, 0, 0.2, 1) ${i % 6 * 0.06}s, transform 0.6s cubic-bezier(0.4, 0, 0.2, 1) ${i % 6 * 0.06}s`;
    io.observe(el);
  });

  // CSS class to complete animation
  const style = document.createElement('style');
  style.textContent = `.is-visible { opacity: 1 !important; transform: translateY(0) !important; }`;
  document.head.appendChild(style);

  /* --- Smooth header background on scroll --- */
  const header = document.querySelector('.site-header');
  if (header) {
    let ticking = false;
    window.addEventListener('scroll', () => {
      if (!ticking) {
        window.requestAnimationFrame(() => {
          if (window.scrollY > 80) {
            header.style.borderBottomColor = 'rgba(130, 80, 220, 0.18)';
            header.style.background = 'rgba(6, 3, 15, 0.92)';
          } else {
            header.style.borderBottomColor = '';
            header.style.background = '';
          }
          ticking = false;
        });
        ticking = true;
      }
    });
  }

  /* --- Copy install command on click --- */
  const installCodes = document.querySelectorAll('.hero-install');
  installCodes.forEach((el) => {
    el.style.cursor = 'pointer';
    el.title = 'Click to copy';
    el.addEventListener('click', () => {
      const text = el.textContent.replace('$', '').trim();
      navigator.clipboard.writeText(text).then(() => {
        const original = el.innerHTML;
        el.textContent = '✓ Copied!';
        el.style.color = '#22c55e';
        setTimeout(() => {
          el.innerHTML = original;
          el.style.color = '';
        }, 1600);
      });
    });
  });

  /* --- Parallax on glow orbs (subtle, performance-safe) --- */
  const glows = document.querySelectorAll('.bg-glow');
  if (glows.length && window.matchMedia('(prefers-reduced-motion: no-preference)').matches) {
    let rafId;
    window.addEventListener('mousemove', (e) => {
      if (rafId) cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(() => {
        const x = (e.clientX / window.innerWidth - 0.5) * 120;
        const y = (e.clientY / window.innerHeight - 0.5) * 120;
        glows.forEach((g, i) => {
          const factor = i === 0 ? 1 : -0.7;
          g.style.transform = `translate(${x * factor}px, ${y * factor}px)`;
        });
      });
    });
  }

})();
