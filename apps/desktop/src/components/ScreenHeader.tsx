import type { ReactNode } from "react";

interface ScreenHeaderProps {
  title: string;
  subtitle: string;
  actions?: ReactNode;
}

/** The title, subtitle and primary actions at the top of every screen. */
export function ScreenHeader({ title, subtitle, actions }: ScreenHeaderProps) {
  return (
    <div className="screen-header">
      <div>
        <h1 className="screen-header__title">{title}</h1>
        <p className="screen-header__sub">{subtitle}</p>
      </div>
      {actions}
    </div>
  );
}
