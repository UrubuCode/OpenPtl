import { Home, KeyRound, NotebookTabs, Settings2, StickyNote } from "lucide-react";

import { useT } from "@/langs";
import { cn } from "@/lib/utils";
import type { SidebarSection } from "@/types/workspace";

interface BottomNavProps {
  current: SidebarSection;
  onSelect: (section: SidebarSection) => void;
}

// Mobile-only bottom tab bar replacing the desktop sidebar. Shows the primary
// sections; settings/about are reachable from the last slot.
export function BottomNav({ current, onSelect }: BottomNavProps) {
  const t = useT();

  const items: Array<{ id: SidebarSection; label: string; Icon: typeof Home }> = [
    { id: "home", label: t.sidebar.home, Icon: Home },
    { id: "keychain", label: t.sidebar.keychain, Icon: KeyRound },
    { id: "known_hosts", label: t.sidebar.knownHosts, Icon: NotebookTabs },
    { id: "notes", label: t.sidebar.notes, Icon: StickyNote },
    { id: "settings", label: t.sidebar.settings, Icon: Settings2 },
  ];

  // Treat about/debug_logs as part of the settings slot so it stays highlighted.
  const activeId: SidebarSection =
    current === "about" || current === "debug_logs" ? "settings" : current;

  return (
    <nav
      className="flex shrink-0 items-stretch justify-around border-t border-border/60 bg-background/95 pb-[env(safe-area-inset-bottom)] backdrop-blur"
      aria-label={t.sidebar.home}
    >
      {items.map(({ id, label, Icon }) => {
        const active = activeId === id;
        return (
          <button
            key={id}
            type="button"
            onClick={() => onSelect(id)}
            aria-current={active ? "page" : undefined}
            className={cn(
              "flex flex-1 flex-col items-center gap-1 py-2 text-[10px] font-medium transition-colors",
              active ? "text-primary" : "text-muted-foreground",
            )}
          >
            <Icon className="h-5 w-5" aria-hidden />
            <span className="max-w-full truncate">{label}</span>
          </button>
        );
      })}
    </nav>
  );
}
