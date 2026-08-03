import type { ReactNode } from "react";

interface SettingRowProps {
  label: string;
  description?: string;
  children: ReactNode;
}

export function SettingRow({ label, description, children }: SettingRowProps) {
  return (
    <div className="setting-row">
      <div className="setting-copy">
        <span className="setting-label">{label}</span>
        {description ? <span className="setting-description">{description}</span> : null}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}
