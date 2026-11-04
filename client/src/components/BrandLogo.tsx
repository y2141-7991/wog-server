/**
 * The WoG logo, drawn inline so its ink follows the theme (`--logo-ink`): the
 * original navy would disappear on the dark background. The green bar keeps
 * its colour. The same drawing, in navy, is `public/logo.svg` (the favicon).
 */
export function BrandLogo({ size = 32, className }: { size?: number; className?: string }) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="55 37 500 500"
      aria-hidden="true"
      focusable="false"
    >
      <rect x="249" y="184" width="112" height="22" rx="11" fill="#4cbf8b" />
      <circle
        cx="305"
        cy="287"
        r="227"
        fill="none"
        strokeWidth="25"
        style={{ stroke: 'var(--logo-ink)' }}
      />
      <path
        d="M143 128 L222 498 L305 197 L388 498 L467 128 M305 197 L305 514"
        fill="none"
        strokeWidth="22"
        strokeLinecap="round"
        strokeLinejoin="round"
        style={{ stroke: 'var(--logo-ink)' }}
      />
    </svg>
  )
}

/** The page heading on the signed-out screens: the logo, with "WoG" for screen readers. */
export function BrandHeading() {
  return (
    <h1 className="brand-heading">
      <BrandLogo size={84} />
      <span className="sr-only">WoG</span>
    </h1>
  )
}
