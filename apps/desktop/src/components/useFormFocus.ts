import { useEffect, useRef, useState, type RefObject } from "react";

/**
 * Focus for a form that opens in place of the button that opened it: focus
 * moves into the form when it opens, and back to that button when it closes,
 * so keyboard and screen reader users keep their place. When the button was
 * itself hidden while the form was open, the element with the id `returnTo`
 * is focused instead.
 */
export function useFormFocus<T extends HTMLElement>(returnTo?: string): RefObject<T | null> {
  const target = useRef<T>(null);
  // Read while rendering, before the opening button can leave the page.
  const [opener] = useState(() => document.activeElement);

  useEffect(() => {
    target.current?.focus();
    return () => {
      const back =
        opener instanceof HTMLElement && opener.isConnected
          ? opener
          : returnTo && document.getElementById(returnTo);
      if (back) {
        back.focus();
      }
    };
  }, [opener, returnTo]);

  return target;
}
