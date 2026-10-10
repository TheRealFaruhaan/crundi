/**
 * The Crundi mark, as markup.
 *
 * Mini (the whole robot) is the logo. Its head alone stands in wherever the
 * mark is drawn at 48 pixels or less, because a body that small is mush. The
 * name is drawn from strokes rather than set in a font, so it looks the same
 * on every machine.
 *
 * These are strings so the page can carry them inline: an inline mark paints
 * with the first byte of HTML (the start-up screen depends on that) and can be
 * animated with CSS. Every call takes an id prefix, since two copies of the
 * same gradient or mask id on one page fight each other.
 *
 * Colours: the body is the app's own indigo (--accent-hover down to
 * --accent-deep), the small parts are amber. Amber is the one warm colour in a
 * cool interface, which is the point of it.
 */

export const BRAND = {
  indigoLight: '#818cf8',
  indigoDeep: '#4f46e5',
  amber: '#fbbf24',
  amberOnLight: '#d97706',
};

const STROKE = 'fill="none" stroke-linecap="round" stroke-linejoin="round"';

function gradient(id) {
  return '<linearGradient id="' + id + '" gradientUnits="userSpaceOnUse" x1="14" y1="14" x2="82" y2="82">'
    + '<stop offset="0" stop-color="' + BRAND.indigoLight + '"/><stop offset="1" stop-color="' + BRAND.indigoDeep + '"/></linearGradient>';
}

/**
 * The whole robot, 96 by 96.
 * Parts carry classes (cb-head, cb-eye, cb-tip, cb-arm, cb-light) so a
 * stylesheet can move them; with no stylesheet it is simply the still logo.
 */
export function miniSvg(p, cls = '', label = '') {
  const g = p + 'g', e = p + 'e', l = p + 'l';
  return '<svg class="cb-mini ' + cls + '" viewBox="0 0 96 96" ' + (label ? 'role="img" aria-label="' + label + '"' : 'aria-hidden="true"') + '>'
    + '<defs>' + gradient(g)
    + '<mask id="' + e + '"><rect width="96" height="96" fill="#fff"/>'
    + '<rect class="cb-eye" x="36.5" y="27" width="7.5" height="13" rx="3.7" fill="#000"/>'
    + '<rect class="cb-eye" x="52" y="27" width="7.5" height="13" rx="3.7" fill="#000"/></mask>'
    + '<mask id="' + l + '"><rect width="96" height="96" fill="#fff"/><circle class="cb-light" cx="48" cy="73" r="5.5" fill="#000"/></mask></defs>'
    + '<g class="cb-head"><path d="M48 16V10" ' + STROKE + ' stroke="' + BRAND.amber + '" stroke-width="4"/>'
    + '<circle class="cb-tip" cx="48" cy="7.5" r="4.5" fill="' + BRAND.amber + '"/>'
    + '<rect x="23" y="15" width="50" height="38" rx="15" fill="url(#' + g + ')" mask="url(#' + e + ')"/></g>'
    + '<rect class="cb-arm cb-arm-l" x="19" y="60" width="7" height="18" rx="3.5" fill="' + BRAND.amber + '"/>'
    + '<rect class="cb-arm cb-arm-r" x="70" y="60" width="7" height="18" rx="3.5" fill="' + BRAND.amber + '"/>'
    + '<rect class="cb-body" x="31" y="58" width="34" height="30" rx="10" fill="url(#' + g + ')" mask="url(#' + l + ')"/>'
    + '</svg>';
}

/** The head alone, for small sizes. */
export function headSvg(p, cls = '') {
  const g = p + 'g', e = p + 'e';
  return '<svg class="cb-headmark ' + cls + '" viewBox="0 0 96 96" aria-hidden="true">'
    + '<defs>' + gradient(g)
    + '<mask id="' + e + '"><rect width="96" height="96" fill="#fff"/>'
    + '<rect class="cb-eye" x="32" y="43" width="10.5" height="18" rx="5.2" fill="#000"/>'
    + '<rect class="cb-eye" x="53.5" y="43" width="10.5" height="18" rx="5.2" fill="#000"/></mask></defs>'
    + '<path d="M48 27V17" ' + STROKE + ' stroke="' + BRAND.amber + '" stroke-width="5.5"/>'
    + '<circle class="cb-tip" cx="48" cy="13" r="6.5" fill="' + BRAND.amber + '"/>'
    + '<rect x="11" y="26" width="74" height="57" rx="23" fill="url(#' + g + ')" mask="url(#' + e + ')"/>'
    + '</svg>';
}

/**
 * The name, drawn: six letters from one circle and one stem. It takes the
 * text colour of wherever it sits; the dot of the i is amber.
 */
export function wordSvg(cls = '') {
  return '<svg class="cb-word ' + cls + '" viewBox="-2 -2 279 70" role="img" aria-label="Crundi">'
    + '<g ' + STROKE + ' stroke="currentColor" stroke-width="10">'
    + '<path pathLength="1" d="M35.23 54.73A18 18 0 1 1 35.23 29.27"/>'
    + '<path pathLength="1" d="M54 60V24M54 42A18 18 0 0 1 78.16 25.09"/>'
    + '<path pathLength="1" d="M95 24V42A18 18 0 0 0 131 42M131 24V60"/>'
    + '<path pathLength="1" d="M153 60V24M153 42A18 18 0 0 1 189 42V60"/>'
    + '<path pathLength="1" d="M247 42A18 18 0 1 1 211 42A18 18 0 1 1 247 42M247 6V60"/>'
    + '<path pathLength="1" d="M269 24V60"/></g>'
    + '<circle class="cb-dot" cx="269" cy="8.5" r="6" fill="' + BRAND.amber + '"/></svg>';
}

/**
 * How the mark moves. One stylesheet for every page that shows it.
 *   .cb-load   the loading loop: nods, blinks, waves, the light pulses
 *   .cb-intro  plays once: the robot lands, blinks, waves; the name is traced
 *   .cb-idle   barely alive: a blink now and then
 * Kept free of backticks and dollar-brace so it can sit inside the page's
 * template literal.
 */
export const BRAND_CSS = [
  '.cb-mini,.cb-headmark,.cb-word{display:block}',
  '.cb-mini *,.cb-headmark *{transform-box:fill-box;transform-origin:center}',
  '.cb-mini .cb-arm{transform-origin:center top}',
  '@keyframes cb-nod{0%,100%{transform:none}50%{transform:translateY(-3px)}}',
  '@keyframes cb-blink{0%,40%,48%,100%{transform:scaleY(1)}44%{transform:scaleY(.1)}}',
  '@keyframes cb-pulse{0%,100%{transform:scale(1)}50%{transform:scale(.7)}}',
  '@keyframes cb-wave{0%,100%{transform:rotate(0)}50%{transform:rotate(-28deg)}}',
  '@keyframes cb-land{0%{transform:translateY(16px) scale(.7);opacity:0}70%{transform:translateY(-3px) scale(1.03);opacity:1}100%{transform:none;opacity:1}}',
  '@keyframes cb-draw{0%{stroke-dashoffset:1;opacity:0}4%{opacity:1}100%{stroke-dashoffset:0;opacity:1}}',
  '@keyframes cb-pop{0%{transform:scale(0);opacity:0}70%{transform:scale(1.2);opacity:1}100%{transform:scale(1);opacity:1}}',
  '.cb-load .cb-head{animation:cb-nod 1.2s ease-in-out infinite}',
  '.cb-load .cb-eye{animation:cb-blink 3.2s ease-in-out infinite}',
  '.cb-load .cb-tip,.cb-load .cb-light{animation:cb-pulse 1.2s ease-in-out infinite}',
  '.cb-load .cb-arm-r{animation:cb-wave .6s ease-in-out infinite}',
  '.cb-idle .cb-eye{animation:cb-blink 5s ease-in-out infinite}',
  '.cb-intro.cb-mini{transform-origin:50% 62%;animation:cb-land .6s ease-out both}',
  '.cb-intro .cb-eye{animation:cb-blink 1.4s ease-in-out .5s both,cb-blink 5s ease-in-out 2.4s infinite}',
  '.cb-intro .cb-arm-r{animation:cb-wave .5s ease-in-out .7s 2 both}',
  '.cb-word.cb-intro path{stroke-dasharray:1;animation:cb-draw .5s ease-out both}',
  '.cb-word.cb-intro path:nth-of-type(1){animation-delay:.35s}.cb-word.cb-intro path:nth-of-type(2){animation-delay:.45s}',
  '.cb-word.cb-intro path:nth-of-type(3){animation-delay:.55s}.cb-word.cb-intro path:nth-of-type(4){animation-delay:.65s}',
  '.cb-word.cb-intro path:nth-of-type(5){animation-delay:.75s}.cb-word.cb-intro path:nth-of-type(6){animation-delay:.85s}',
  '.cb-word.cb-intro .cb-dot{transform-box:fill-box;transform-origin:center;animation:cb-pop .3s ease-out 1.2s both}',
  '@media (prefers-reduced-motion:reduce){.cb-mini,.cb-mini *,.cb-headmark *,.cb-word *{animation:none!important}}',
].join('\n');
