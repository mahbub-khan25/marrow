// A small right-click menu.

import { $, h, icon } from "./util";

export type MenuItem = { label: string; icon: string; run: () => void; danger?: boolean } | "-";

export function showMenu(x: number, y: number, items: MenuItem[]) {
  const menu = $("context-menu");
  menu.replaceChildren(
    ...items.map((it) =>
      it === "-"
        ? h("hr")
        : h(
            `button${it.danger ? ".danger" : ""}` as "button",
            {
              role: "menuitem",
              onclick: () => {
                hideMenu();
                it.run();
              },
            },
            icon(it.icon),
            it.label,
          ),
    ),
  );
  menu.onkeydown = (e) => {
    const buttons = [...menu.querySelectorAll<HTMLButtonElement>("button")];
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      buttons[(i + step + buttons.length) % buttons.length]?.focus();
    } else if (e.key === "Escape") {
      e.stopPropagation();
      hideMenu();
    }
  };
  menu.hidden = false;
  const r = menu.getBoundingClientRect();
  menu.style.left = `${Math.max(4, Math.min(x, innerWidth - r.width - 8))}px`;
  menu.style.top = `${Math.max(4, Math.min(y, innerHeight - r.height - 8))}px`;
  menu.querySelector<HTMLElement>("button")?.focus();
}

export function hideMenu() {
  $("context-menu").hidden = true;
}

document.addEventListener("mousedown", (e) => {
  if (!(e.target as HTMLElement).closest("#context-menu")) hideMenu();
});
window.addEventListener("blur", hideMenu);
