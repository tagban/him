// Smileys: typed text codes drawn as little yellow faces, the way AIM showed them.
// Messages still travel as plain text (so other Hotline clients see ":-)"); only
// the display changes. The faces are drawn here, for HIM.

const Smileys = (() => {
  // Face: yellow disc with a dark outline; each smiley adds its own features.
  const face = (extra, { fill = 'url(#g)' } = {}) =>
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" width="20" height="20">` +
    `<defs><radialGradient id="g" cx="0.38" cy="0.32" r="0.75"><stop offset="0" stop-color="#fff7a0"/><stop offset="0.45" stop-color="#ffe21a"/><stop offset="1" stop-color="#e8a800"/></radialGradient></defs>` +
    `<circle cx="10" cy="10.5" r="8.4" fill="${fill}" stroke="#5a3c00" stroke-width="1.1"/>${extra}</svg>`;

  const eyes = `<ellipse cx="7.1" cy="8" rx="1.05" ry="1.6" fill="#2a1a00"/><ellipse cx="12.9" cy="8" rx="1.05" ry="1.6" fill="#2a1a00"/>`;
  const smile = `<path d="M5.6 11.6 Q10 16.2 14.4 11.6" fill="none" stroke="#2a1a00" stroke-width="1.3" stroke-linecap="round"/>`;
  const line = (d, w = 1.3, c = '#2a1a00') => `<path d="${d}" fill="none" stroke="${c}" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"/>`;

  // name, the codes that make it (the first is what the picker inserts), the drawing
  const SET = [
    ['Smile', [':-)', ':)'], face(eyes + smile)],
    ['Frown', [':-(', ':('], face(eyes + line('M6 14.4 Q10 10.8 14 14.4'))],
    ['Wink', [';-)', ';)'], face(line('M5.9 8.3 Q7.1 7.2 8.3 8.3', 1.2) + `<ellipse cx="12.9" cy="8" rx="1.05" ry="1.6" fill="#2a1a00"/>` + smile)],
    ['Sticking out tongue', [':-P', ':P', ':-p', ':p'], face(eyes + line('M5.8 12 Q10 14.6 14.2 12') + `<path d="M8.4 13.1 Q8.6 17 10.6 16.6 Q12.2 16.2 11.9 13.3 Z" fill="#e8434b" stroke="#7a1418" stroke-width="0.7"/>`)],
    ['Laughing', [':-D', ':D'], face(eyes + `<path d="M5.4 11 Q10 11.9 14.6 11 Q13.9 16.4 10 16.4 Q6.1 16.4 5.4 11 Z" fill="#6b1010" stroke="#2a1a00" stroke-width="1"/><path d="M6.2 11.5 Q10 12.3 13.8 11.5 L13.5 12.6 Q10 13.2 6.5 12.6 Z" fill="#fff"/>`)],
    ['Surprised', ['=-O', ':-O', ':O', '=O'], face(eyes + `<ellipse cx="10" cy="13.4" rx="1.8" ry="2.3" fill="#4a0e0e" stroke="#2a1a00" stroke-width="0.9"/>`)],
    ['Kissing', [':-*', ':*'], face(line('M5.9 8.4 Q7.1 7.3 8.3 8.4', 1.2) + line('M11.7 8.4 Q12.9 7.3 14.1 8.4', 1.2) + `<path d="M9.2 12.3 Q10.8 11.5 10.4 13 Q12.1 13.2 10.6 14.2 Q10 14.6 9.2 14.1 Q10.1 13.3 9.2 12.3 Z" fill="#e8434b" stroke="#7a1418" stroke-width="0.6"/>`)],
    ['Yelling', ['>:o', '>:O', '>:-o'], face(line('M5.4 6 L8.4 7.4', 1.2) + line('M14.6 6 L11.6 7.4', 1.2) + `<ellipse cx="7.2" cy="8.9" rx="0.9" ry="1.2" fill="#2a1a00"/><ellipse cx="12.8" cy="8.9" rx="0.9" ry="1.2" fill="#2a1a00"/><ellipse cx="10" cy="14" rx="3.2" ry="2.4" fill="#4a0e0e" stroke="#2a1a00" stroke-width="0.9"/><path d="M8.1 15.4 Q10 14.4 11.9 15.4" fill="#e8434b"/>`)],
    ['Cool', ['8-)', 'B-)'], face(`<path d="M3.2 6.8 H16.8" stroke="#111" stroke-width="1.1"/><path d="M3.8 6.9 H9 Q9 10.3 6.4 10.3 Q3.9 10.3 3.8 6.9 Z M11 6.9 H16.2 Q16.1 10.3 13.6 10.3 Q11 10.3 11 6.9 Z" fill="#111"/><path d="M4.7 7.6 L6 7.6" stroke="#7a7a7a" stroke-width="0.7"/><path d="M11.9 7.6 L13.2 7.6" stroke="#7a7a7a" stroke-width="0.7"/>` + smile)],
    ['Money-mouth', [':-$', ':$'], face(eyes + `<path d="M5.6 11.6 Q10 16.8 14.4 11.6 Z" fill="#2e8b3a" stroke="#1a4a1e" stroke-width="0.9"/><text x="10" y="15.2" font-size="4.6" font-family="Arial" font-weight="bold" text-anchor="middle" fill="#d8ffd0">$</text>`)],
    ['Foot-in-mouth', [':-!', ':!'], face(eyes + `<ellipse cx="10" cy="13.4" rx="2.4" ry="1.9" fill="#4a0e0e"/><path d="M8.4 12.2 Q12.8 10.6 15.8 12.6 Q17.2 14.4 15.4 15.2 Q12.4 15.8 9.2 14.4 Z" fill="#7a4a1c" stroke="#3a1e06" stroke-width="0.7"/>`)],
    ['Embarrassed', [':-[', ':['], face(eyes + `<ellipse cx="5.4" cy="11.6" rx="1.7" ry="1" fill="#ff7a7a" opacity="0.8"/><ellipse cx="14.6" cy="11.6" rx="1.7" ry="1" fill="#ff7a7a" opacity="0.8"/>` + line('M7.8 13.8 Q10 12.9 12.2 13.8'))],
    ['Innocent', ['O:-)', 'O:)', '0:-)'], face(eyes + smile + `<ellipse cx="10" cy="2.3" rx="5.2" ry="1.5" fill="none" stroke="#8a6a00" stroke-width="2"/><ellipse cx="10" cy="2.3" rx="5.2" ry="1.5" fill="none" stroke="#fff27a" stroke-width="1.1"/>`)],
    ['Undecided', [':-\\', ':-/'], face(eyes + line('M6.2 13.9 L13.8 12'))],
    ['Crying', [":'(", ":'-("], face(eyes + line('M6 14.6 Q10 11 14 14.6') + `<path d="M6.4 9.8 Q5.3 12 6.4 12.7 Q7.5 12 6.4 9.8 Z" fill="#5ab4ff" stroke="#1e6cc0" stroke-width="0.5"/>`)],
    ['Lips are sealed', [':-X', ':X', ':-x'], face(eyes + line('M6.3 12.2 L13.7 12.2', 1.1) + `<path d="M7.5 11 L8.8 13.4 M8.8 11 L7.5 13.4 M11.2 11 L12.5 13.4 M12.5 11 L11.2 13.4" stroke="#2a1a00" stroke-width="0.8"/>`)],
    ['Heart', ['<3'], `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" width="20" height="20"><path d="M10 17.2 C3 12.4 1.6 8.6 3.6 5.6 C5.6 2.8 9 3.6 10 6.2 C11 3.6 14.4 2.8 16.4 5.6 C18.4 8.6 17 12.4 10 17.2 Z" fill="#e8283a" stroke="#6a0a12" stroke-width="1"/><path d="M5 6.6 Q5.8 5.2 7.4 5.4" stroke="#ffb0b8" stroke-width="1" fill="none" stroke-linecap="round"/></svg>`],
  ];

  const url = svg => 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
  const byCode = new Map();
  for (const [name, codes, svg] of SET) for (const c of codes) byCode.set(c, { name, src: url(svg) });

  // Longest codes first, so ">:o" wins over ":o" and "O:-)" over ":-)".
  const codes = [...byCode.keys()].sort((a, b) => b.length - a.length);
  const escRe = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  // A code counts when it stands on its own: at the start or after a space, and
  // before the end, a space or punctuation. That keeps "http://x" and "(a:b)" alone.
  const re = new RegExp(`(^|\\s)(${codes.map(escRe).join('|')})(?=$|\\s|[.,!?;)])`, 'g');

  function img(code) {
    const s = byCode.get(code);
    return `<img class="smiley" src="${s.src}" alt="${esc(code)}" title="${esc(s.name)}  ${esc(code)}">`;
  }

  const URL_RE = /\b(?:https?:\/\/|www\.)[^\s<>"']+/gi;

  function link(url) {
    const href = /^www\./i.test(url) ? 'https://' + url : url;
    return `<a class="url" href="#" data-url="${esc(href)}" title="${esc(href)}">${esc(url)}</a>`;
  }

  function faces(text, on) {
    if (!on) return esc(text);
    let out = '', last = 0, m;
    re.lastIndex = 0;
    while ((m = re.exec(text))) {
      const start = m.index + m[1].length;
      out += esc(text.slice(last, start)) + img(m[2]);
      last = start + m[2].length;
    }
    return out + esc(text.slice(last));
  }

  return {
    /// Plain text to safe HTML: web addresses become links (found first, so the ":/" in
    /// "http://" isn't taken for a smiley), and smiley codes are drawn as pictures.
    html(text, on = true) {
      let out = '', last = 0, u;
      URL_RE.lastIndex = 0;
      while ((u = URL_RE.exec(text))) {
        let url = u[0];
        // Punctuation that ends a sentence isn't part of the address.
        for (;;) {
          const c = url.slice(-1);
          const opens = url.split('(').length, closes = url.split(')').length;
          if (/[.,;:!?'"\]]/.test(c) || (c === ')' && closes > opens)) url = url.slice(0, -1);
          else break;
        }
        out += faces(text.slice(last, u.index), on) + link(url);
        last = u.index + url.length;
        URL_RE.lastIndex = last;
      }
      return out + faces(text.slice(last), on);
    },
    /// The picker: [name, code to insert, picture url] per smiley.
    list: SET.map(([name, codes, svg]) => [name, codes[0], url(svg)]),
  };
})();
