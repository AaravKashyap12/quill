import { CircleCheck, CircleX, LoaderCircle } from "lucide-react";

interface ProviderBadgeProps {
  state: "idle" | "checking" | "available" | "unavailable";
  label: string;
}

export function ProviderBadge({ state, label }: ProviderBadgeProps) {
  const Icon =
    state === "checking" ? LoaderCircle : state === "available" ? CircleCheck : CircleX;
  return (
    <span className={`provider-state is-${state}`}>
      {state === "idle" ? null : <Icon size={14} aria-hidden="true" />}
      {label}
    </span>
  );
}
