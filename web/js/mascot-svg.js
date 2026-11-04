// Mitthu the parrot. Drawn by hand in a 240 x 270 box so CSS can move the parts.
// Colours that should follow the theme go through `style`, because CSS variables
// don't work in SVG presentation attributes.
export const MITTHU_SVG = `
<svg class="mitthu" viewBox="0 0 240 270" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Mitthu, a green parrot">
  <defs>
    <radialGradient id="mt-glow" cx="50%" cy="48%" r="50%">
      <stop offset="0" style="stop-color: var(--leaf-100)"/>
      <stop offset="1" style="stop-color: var(--leaf-100); stop-opacity: 0"/>
    </radialGradient>
    <linearGradient id="mt-body" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#84c044"/><stop offset="1" stop-color="#5ea32a"/>
    </linearGradient>
    <linearGradient id="mt-head" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#94d153"/><stop offset="1" stop-color="#7fbd3f"/>
    </linearGradient>
    <linearGradient id="mt-wing" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#4f9223"/><stop offset="1" stop-color="#3a7219"/>
    </linearGradient>
  </defs>

  <circle class="glow" cx="120" cy="132" r="114" fill="url(#mt-glow)"/>

  <g class="bird">
    <g class="tail">
      <path d="M104 196 C 92 226, 90 248, 96 266 C 108 250, 116 224, 118 196 Z" fill="#ffc531"/>
      <path d="M124 196 C 130 230, 142 252, 160 262 C 160 242, 150 218, 140 194 Z" fill="#e5483a"/>
      <path d="M114 192 C 112 232, 118 256, 128 268 C 136 250, 136 222, 130 190 Z" fill="#2fb5a6"/>
    </g>

    <path class="wing-back" d="M170 128 C 192 148, 192 192, 162 214 C 150 196, 148 160, 156 132 Z" fill="#4a8a22"/>
    <ellipse cx="120" cy="156" rx="64" ry="70" fill="url(#mt-body)"/>
    <ellipse cx="120" cy="174" rx="39" ry="48" fill="#cde8a4"/>
    <path d="M102 164 q8 8 16 0 M122 164 q8 8 16 0 M112 184 q8 8 16 0 M96 188 q8 8 16 0 M128 188 q8 8 16 0"
          fill="none" stroke="#a8d472" stroke-width="3" stroke-linecap="round" opacity=".75"/>

    <g class="wing">
      <path d="M70 126 C 42 150, 46 198, 84 218 C 96 198, 98 160, 88 130 Z" fill="url(#mt-wing)"/>
      <path d="M58 168 C 60 184, 70 202, 84 216" fill="none" stroke="#2fb5a6" stroke-width="6" stroke-linecap="round"/>
      <path d="M52 150 C 52 164, 58 178, 66 190" fill="none" stroke="#ffc531" stroke-width="5" stroke-linecap="round"/>
      <path d="M74 136 C 80 156, 84 176, 82 196" fill="none" stroke="#3a7219" stroke-width="3" stroke-linecap="round" opacity=".6"/>
    </g>

    <g class="perch">
      <rect x="20" y="222" width="200" height="16" rx="8" fill="#c48d54"/>
      <rect x="20" y="231" width="200" height="7" rx="3.5" fill="#a67238"/>
      <circle cx="56" cy="229" r="2.6" fill="#8d5f2c"/>
      <circle cx="184" cy="228" r="2" fill="#8d5f2c"/>
      <path d="M30 226 q10 -3 20 0 M160 226 q10 -3 20 0" stroke="#d7a56d" stroke-width="2" fill="none" stroke-linecap="round" opacity=".8"/>
    </g>
    <g class="feet" fill="#ff8a3d">
      <ellipse cx="100" cy="222" rx="10" ry="5.5"/><circle cx="91" cy="224.5" r="3.2"/><circle cx="100" cy="227.5" r="3.2"/><circle cx="109" cy="224.5" r="3.2"/>
      <ellipse cx="140" cy="222" rx="10" ry="5.5"/><circle cx="131" cy="224.5" r="3.2"/><circle cx="140" cy="227.5" r="3.2"/><circle cx="149" cy="224.5" r="3.2"/>
    </g>

    <g class="head">
      <path class="crest c1" d="M106 46 C 96 34, 96 22, 104 16 C 108 28, 112 36, 114 48 Z" fill="#ff8a3d"/>
      <path class="crest c2" d="M120 42 C 112 28, 116 14, 126 8 C 130 22, 128 34, 126 46 Z" fill="#e5483a"/>
      <path class="crest c3" d="M134 46 C 144 34, 146 24, 140 16 C 134 26, 130 36, 128 48 Z" fill="#ffc531"/>
      <circle cx="120" cy="86" r="50" fill="url(#mt-head)"/>
      <ellipse cx="120" cy="94" rx="38" ry="31" fill="#f7fbec"/>
      <circle class="cheek" cx="90" cy="105" r="8" fill="#f2699b"/>
      <circle class="cheek" cx="150" cy="105" r="8" fill="#f2699b"/>

      <g class="eye">
        <g class="ball">
          <circle cx="101" cy="86" r="12.5" fill="#fff" stroke="#d9e6c3" stroke-width="1.5"/>
          <g class="pupil">
            <circle cx="101" cy="87" r="7.4" fill="#17181c"/>
            <circle cx="98.4" cy="84" r="2.6" fill="#fff"/>
            <circle cx="103.6" cy="90" r="1.3" fill="#fff" opacity=".8"/>
          </g>
          <rect class="lid" x="86" y="72" width="30" height="30" rx="15" fill="#f7fbec"/>
        </g>
        <path class="joy" d="M90 90 q11 -15 22 0" fill="none" stroke="#17181c" stroke-width="4.2" stroke-linecap="round"/>
      </g>
      <g class="eye">
        <g class="ball">
          <circle cx="139" cy="86" r="12.5" fill="#fff" stroke="#d9e6c3" stroke-width="1.5"/>
          <g class="pupil">
            <circle cx="139" cy="87" r="7.4" fill="#17181c"/>
            <circle cx="136.4" cy="84" r="2.6" fill="#fff"/>
            <circle cx="141.6" cy="90" r="1.3" fill="#fff" opacity=".8"/>
          </g>
          <rect class="lid" x="124" y="72" width="30" height="30" rx="15" fill="#f7fbec"/>
        </g>
        <path class="joy" d="M128 90 q11 -15 22 0" fill="none" stroke="#17181c" stroke-width="4.2" stroke-linecap="round"/>
      </g>
      <path class="brow l" d="M88 67 q13 -7 26 0" fill="none" stroke="#3f6a1b" stroke-width="3.6" stroke-linecap="round"/>
      <path class="brow r" d="M126 67 q13 -7 26 0" fill="none" stroke="#3f6a1b" stroke-width="3.6" stroke-linecap="round"/>

      <path class="mouth" d="M111 119 C 112 135, 128 135, 129 119 Z" fill="#6b2a1b"/>
      <g class="jaw"><path d="M108 119 C 110 135, 130 135, 132 119 Z" fill="#e8742a"/></g>
      <path class="beak" d="M103 99 C 103 90, 137 90, 137 99 C 137 113, 129 127, 120 130 C 111 127, 103 113, 103 99 Z" fill="#ff9a3d"/>
      <path d="M111 99 C 113 95, 120 94, 125 96" stroke="#ffc88a" stroke-width="3" fill="none" stroke-linecap="round"/>
      <circle cx="113" cy="104" r="1.7" fill="#d9692a"/><circle cx="127" cy="104" r="1.7" fill="#d9692a"/>

      <path class="fx sweat" d="M162 50 q-8 11 0 16 q8 -5 0 -16 Z" fill="#7cc4f4"/>
    </g>

    <g class="fx think">
      <circle class="d1" cx="172" cy="54" r="3.4" style="fill: var(--leaf-300)"/>
      <circle class="d2" cx="182" cy="44" r="4.6" style="fill: var(--leaf-300)"/>
      <g transform="translate(200 24)">
        <ellipse rx="23" ry="15" style="fill: var(--card); stroke: var(--leaf-300)" stroke-width="2"/>
        <circle class="t1" cx="-9" cy="1" r="2.6" style="fill: var(--leaf-600)"/>
        <circle class="t2" cx="0" cy="1" r="2.6" style="fill: var(--leaf-600)"/>
        <circle class="t3" cx="9" cy="1" r="2.6" style="fill: var(--leaf-600)"/>
      </g>
    </g>
    <g class="fx hear" fill="none" stroke-linecap="round" style="stroke: var(--leaf-500)" stroke-width="4">
      <path class="a1" d="M60 78 q-9 9 0 20"/>
      <path class="a2" d="M50 70 q-15 17 0 36"/>
      <path class="a3" d="M40 62 q-21 25 0 52"/>
    </g>
    <g class="fx talk" fill="none" stroke-linecap="round" style="stroke: var(--saffron)" stroke-width="4">
      <path class="a1" d="M150 116 q9 7 0 14"/>
      <path class="a2" d="M159 109 q15 14 0 28"/>
    </g>
    <g class="fx sparkles">
      <path class="s1" d="M46 54 l2.4 6.6 l6.6 2.4 l-6.6 2.4 l-2.4 6.6 l-2.4 -6.6 l-6.6 -2.4 l6.6 -2.4 Z" fill="#ffc531"/>
      <path class="s2" d="M196 72 l2 5.6 l5.6 2 l-5.6 2 l-2 5.6 l-2 -5.6 l-5.6 -2 l5.6 -2 Z" fill="#f2699b"/>
      <path class="s3" d="M168 14 l2.4 6.6 l6.6 2.4 l-6.6 2.4 l-2.4 6.6 l-2.4 -6.6 l-6.6 -2.4 l6.6 -2.4 Z" fill="#2fb5a6"/>
      <path class="s4" d="M70 22 l1.8 5 l5 1.8 l-5 1.8 l-1.8 5 l-1.8 -5 l-5 -1.8 l5 -1.8 Z" fill="#ff8a3d"/>
    </g>
  </g>
</svg>`;

// The little head used as the logo in the top bar.
export const MITTHU_MARK = `
<svg class="brand-mark" viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
  <path d="M19 9 C 16 5, 16 2, 18 1 C 20 4, 21 6, 22 9 Z" fill="#ff8a3d"/>
  <path d="M25 8 C 23 3, 25 0, 28 0 C 29 4, 28 7, 27 9 Z" fill="#e5483a"/>
  <circle cx="24" cy="25" r="19" fill="#86c046"/>
  <ellipse cx="24" cy="28" rx="14" ry="11.5" fill="#f7fbec"/>
  <circle cx="17.5" cy="25" r="4.4" fill="#fff"/><circle cx="30.5" cy="25" r="4.4" fill="#fff"/>
  <circle cx="18" cy="25.6" r="2.7" fill="#17181c"/><circle cx="30" cy="25.6" r="2.7" fill="#17181c"/>
  <circle cx="17.2" cy="24.6" r="0.9" fill="#fff"/><circle cx="29.2" cy="24.6" r="0.9" fill="#fff"/>
  <path d="M19.5 29.5 C 19.5 27, 28.5 27, 28.5 29.5 C 28.5 34, 26 38, 24 39 C 22 38, 19.5 34, 19.5 29.5 Z" fill="#ff9a3d"/>
  <circle cx="13" cy="31" r="2.6" fill="#f2699b" opacity=".4"/><circle cx="35" cy="31" r="2.6" fill="#f2699b" opacity=".4"/>
</svg>`;
