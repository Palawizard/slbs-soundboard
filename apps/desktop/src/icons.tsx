import type { ReactNode, SVGProps } from "react";

// One stroke family for the whole interface: 24px grid, 1.6px round strokes.
function Icon({ children, ...props }: SVGProps<SVGSVGElement> & { children: ReactNode }) {
  return (
    <svg className="icon" viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false" {...props}>
      {children}
    </svg>
  );
}

export const SoundsIcon = () => <Icon><rect x="4" y="4" width="6.5" height="6.5" rx="1.5" /><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.5" /><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.5" /><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.5" /></Icon>;
export const AudioIcon = () => <Icon><path d="M12 4.5a2.5 2.5 0 0 1 2.5 2.5v4.5a2.5 2.5 0 0 1-5 0V7A2.5 2.5 0 0 1 12 4.5Z" /><path d="M6.5 11a5.5 5.5 0 0 0 11 0M12 16.5V20" /></Icon>;
export const SettingsIcon = () => <Icon><path d="M5 7h8M17 7h2M5 17h2M11 17h8" /><circle cx="15" cy="7" r="2" /><circle cx="9" cy="17" r="2" /></Icon>;
export const CommunityIcon = () => <Icon><circle cx="9" cy="9" r="3" /><path d="M3.5 19a5.5 5.5 0 0 1 11 0" /><path d="M15.5 6.2a3 3 0 0 1 0 5.6M17.5 14.2A5.5 5.5 0 0 1 20.5 19" /></Icon>;
export const SearchIcon = () => <Icon><circle cx="10.5" cy="10.5" r="5.5" /><path d="m15 15 4.5 4.5" /></Icon>;
export const PlusIcon = () => <Icon><path d="M12 5v14M5 12h14" /></Icon>;
export const StopIcon = () => <Icon><rect x="6.5" y="6.5" width="11" height="11" rx="1.5" /></Icon>;
export const CloseIcon = () => <Icon><path d="m6.5 6.5 11 11M17.5 6.5l-11 11" /></Icon>;
export const ImageIcon = () => <Icon><rect x="4" y="5" width="16" height="14" rx="2" /><circle cx="9" cy="10" r="1.6" /><path d="m5 17 4.5-4.5 3 3 2.5-2.5L19 17" /></Icon>;
export const TrashIcon = () => <Icon><path d="M5 7h14M10 7V5h4v2M7 7l1 12h8l1-12" /></Icon>;
export const ArrowLeftIcon = () => <Icon><path d="M19 12H5M11 6l-6 6 6 6" /></Icon>;
export const ArrowRightIcon = () => <Icon><path d="M5 12h14M13 6l6 6-6 6" /></Icon>;
export const TuneIcon = () => <Icon><path d="M6 4v5M6 13v7M12 4v9M12 17v3M18 4v3M18 11v9" /><path d="M4 11h4M10 15h4M16 9h4" /></Icon>;
export const MoreIcon = () => <Icon><circle cx="6" cy="12" r="1" fill="currentColor" /><circle cx="12" cy="12" r="1" fill="currentColor" /><circle cx="18" cy="12" r="1" fill="currentColor" /></Icon>;
export const RefreshIcon = () => <Icon><path d="M19 8a7.5 7.5 0 1 0 .5 6" /><path d="M19.5 4v4.5H15" /></Icon>;
export const CableIcon = () => <Icon><path d="M8 4v4M12 4v4M6.5 8h7v3.5a3.5 3.5 0 0 1-7 0Z" /><path d="M10 15v2.5a2.5 2.5 0 0 0 2.5 2.5H20" /></Icon>;
export const HeadphonesIcon = () => <Icon><path d="M4.5 15v-3a7.5 7.5 0 0 1 15 0v3" /><rect x="4" y="14" width="4" height="6" rx="1.5" /><rect x="16" y="14" width="4" height="6" rx="1.5" /></Icon>;
export const MixIcon = () => <Icon><path d="M7 4v16M17 4v16M12 4v16" /><rect x="5" y="13" width="4" height="3" rx="1" /><rect x="10" y="7" width="4" height="3" rx="1" /><rect x="15" y="11" width="4" height="3" rx="1" /></Icon>;
export const ChevronIcon = () => <Icon className="icon chevron"><path d="m10 7 5 5-5 5" /></Icon>;
