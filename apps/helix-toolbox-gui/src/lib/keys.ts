// Keyboard helpers the screens share, as Svelte actions: arrow keys through
// a list, and dialogs that take the focus and give it back.

const FOCUSABLE =
  'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [href], [tabindex]:not([tabindex="-1"])';

const shown = (node: HTMLElement) =>
  node.offsetParent !== null || node === document.activeElement;

/**
 * Up and Down (and Home, End) move the focus through the elements of a list
 * that match `items`; those that also match `follow` are selected as the
 * focus reaches them, the way the terminal interface's cursor selects. The
 * list is one stop for Tab: the item last reached, or the selected one.
 */
export function arrows(
  node: HTMLElement,
  options: { items: string; follow?: string },
) {
  let current: HTMLElement | null = null;
  const list = () => [...node.querySelectorAll<HTMLElement>(options.items)];
  function rove() {
    const items = list();
    if (!current || !items.includes(current)) {
      current =
        items.find((item) =>
          item.matches(
            '[aria-current="true"], [aria-current="page"], .selected',
          ),
        ) ??
        items[0] ??
        null;
    }
    for (const item of items) item.tabIndex = item === current ? 0 : -1;
  }
  function onfocusin(event: FocusEvent) {
    const item = list().find(
      (item) => item === event.target || item.contains(event.target as Node),
    );
    if (item && item !== current) {
      current = item;
      rove();
    }
  }
  // Rows come and go as the list is searched, opened or read again.
  const observer = new MutationObserver(rove);
  observer.observe(node, { childList: true, subtree: true });
  rove();

  function onkeydown(event: KeyboardEvent) {
    if (
      !["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key) ||
      event.altKey ||
      event.ctrlKey
    )
      return;
    const items = [...node.querySelectorAll<HTMLElement>(options.items)].filter(
      (item) => shown(item) && !item.hasAttribute("disabled"),
    );
    const index = items.findIndex(
      (item) =>
        item === document.activeElement ||
        item.contains(document.activeElement),
    );
    if (index < 0) return;
    event.preventDefault();
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? items.length - 1
          : Math.min(
              Math.max(index + (event.key === "ArrowDown" ? 1 : -1), 0),
              items.length - 1,
            );
    const item = items[next];
    item.focus();
    item.scrollIntoView({ block: "nearest" });
    if (options.follow && item.matches(options.follow)) item.click();
  }
  node.addEventListener("keydown", onkeydown);
  node.addEventListener("focusin", onfocusin);
  return {
    update(next: { items: string; follow?: string }) {
      options = next;
      rove();
    },
    destroy() {
      observer.disconnect();
      node.removeEventListener("keydown", onkeydown);
      node.removeEventListener("focusin", onfocusin);
    },
  };
}

/**
 * A modal dialog: the focus moves into it (to `[data-autofocus]`, else its
 * first control), Tab and Shift-Tab stay inside it, and when it closes the
 * focus goes back to where it was. In a terminal inside it, Tab goes to the
 * program and Ctrl-Tab moves on.
 */
export function modal(node: HTMLElement) {
  const before = document.activeElement as HTMLElement | null;
  const controls = () =>
    [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(shown);
  const enter = () => {
    if (node.contains(document.activeElement)) return;
    const target =
      node.querySelector<HTMLElement>("[data-autofocus]") ??
      controls()[0] ??
      node;
    target.focus();
  };
  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "Tab") return;
    const list = controls();
    // In the run's terminal, Tab is the program's; Ctrl-Tab leaves it.
    if ((document.activeElement as HTMLElement | null)?.closest(".xterm")) {
      if (!event.ctrlKey) return;
      event.preventDefault();
      const outside = list.filter((control) => !control.closest(".xterm"));
      (event.shiftKey ? outside[outside.length - 1] : outside[0])?.focus();
      return;
    }
    if (list.length === 0) {
      event.preventDefault();
      return;
    }
    const first = list[0];
    const last = list[list.length - 1];
    if (
      event.shiftKey &&
      (document.activeElement === first ||
        !node.contains(document.activeElement))
    ) {
      event.preventDefault();
      last.focus();
    } else if (
      !event.shiftKey &&
      (document.activeElement === last ||
        !node.contains(document.activeElement))
    ) {
      event.preventDefault();
      first.focus();
    }
  }
  // A control that goes away, as a dialog changes its page, leaves the
  // focus on the page's body; it comes back into the dialog.
  const observer = new MutationObserver(() => {
    if (document.activeElement === document.body) enter();
  });
  if (!node.hasAttribute("tabindex")) node.tabIndex = -1;
  queueMicrotask(enter);
  node.addEventListener("keydown", onkeydown);
  observer.observe(node, { childList: true, subtree: true });
  return {
    destroy() {
      observer.disconnect();
      node.removeEventListener("keydown", onkeydown);
      if (before && document.contains(before)) before.focus();
    },
  };
}

/**
 * A region that scrolls but holds no control could not be scrolled from
 * the keyboard: it joins the Tab order while it scrolls, so the arrow keys
 * scroll it, and leaves again when it fits.
 */
export function keepScrollingReachable(root: HTMLElement = document.body) {
  const MARK = "data-scroll-stop";
  let queued = false;
  function check() {
    queued = false;
    for (const node of root.querySelectorAll<HTMLElement>("*")) {
      if (node.closest(".xterm")) continue;
      const style = getComputedStyle(node);
      const scrolls =
        /(auto|scroll)/.test(style.overflowY + style.overflowX) &&
        (node.scrollHeight > node.clientHeight + 1 ||
          node.scrollWidth > node.clientWidth + 1);
      const reachable = scrolls && node.querySelector(FOCUSABLE) === null;
      if (reachable && !node.hasAttribute("tabindex")) {
        node.tabIndex = 0;
        node.setAttribute(MARK, "");
      } else if (!reachable && node.hasAttribute(MARK)) {
        node.removeAttribute("tabindex");
        node.removeAttribute(MARK);
      }
    }
  }
  // At most every 300 ms, and not for the run's terminal drawing output.
  const soon = () => {
    if (queued) return;
    queued = true;
    setTimeout(check, 300);
  };
  new MutationObserver((records) => {
    if (
      records.some(
        (record) =>
          !(
            record.target instanceof Element
              ? record.target
              : record.target.parentElement
          )?.closest(".xterm"),
      )
    )
      soon();
  }).observe(root, { childList: true, subtree: true, characterData: true });
  window.addEventListener("resize", soon);
  soon();
}
