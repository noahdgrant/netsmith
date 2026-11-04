// The netsmith logo: the net-anvil with its hammer, and the wordmark ("net" in the text colour,
// "smith" in the accent). The same drawing as docs/assets/logo-dark.svg, with the editor's colours
// for its fills so a change of theme reaches it. Not a link; the name is its accessible label.
export function Logo() {
  return (
    <svg class="logo" viewBox="0 0 522 128" role="img" aria-label="netsmith" focusable="false">
      <title>netsmith</title>
      <path d="M32.4 70.5H93.6M63 70.5V93.5M63 93.5L47.7 114.9M63 93.5L78.3 114.9M47.7 114.9H78.3" fill="none" stroke="var(--accent)" stroke-width="5.7" />
      <path d="M73.4 31.6L36.6 5.8" fill="none" stroke="var(--accent)" stroke-width="6.7" />
      <g fill="var(--accent)">
        <circle cx="32.4" cy="70.5" r="7.6" />
        <circle cx="63" cy="70.5" r="7.6" />
        <circle cx="93.6" cy="70.5" r="7.6" />
        <circle cx="63" cy="93.5" r="7.6" />
        <circle cx="47.7" cy="114.9" r="7.6" />
        <circle cx="78.3" cy="114.9" r="7.6" />
        <rect x="64.8" y="13.4" width="17.2" height="36.3" rx="3.3" transform="rotate(35 73.4 31.6)" />
      </g>
      <path d="M150 88V40H174L183.6 49.6V88M200.4 64H234V49.6L224.4 40H210L200.4 49.6V78.4L210 88H234M262.8 18.4V78.4L272.4 88H279.6M250.8 40H279.6" fill="none" stroke="var(--text)" stroke-width="8.4" />
      <path d="M330 40H306L296.4 49.6V54.4L306 64H320.4L330 73.6V78.4L320.4 88H296.4M346.8 88V40H361.2L369.6 48.4V88M369.6 48.4L378 40H392.4L400.8 48.4V88M417.6 88V40M446.4 18.4V78.4L456 88H463.2M434.4 40H463.2M480 88V18.4M480 40H504L513.6 49.6V88" fill="none" stroke="var(--accent)" stroke-width="8.4" />
      <circle cx="417.6" cy="24.4" r="5.8" fill="var(--accent)" />
    </svg>
  );
}
