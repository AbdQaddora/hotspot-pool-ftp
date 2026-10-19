import * as React from "react";
import { cn } from "@/lib/utils";

export function Card({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("rounded-xl border border-border bg-card", className)} {...props} />;
}

export function CardHeader({ title, description, icon, right, className }: { title: string; description?: string; icon?: React.ReactNode; right?: React.ReactNode; className?: string }) {
  return (
    <div className={cn("flex items-start gap-3 border-b border-border px-5 py-3.5", className)}>
      {icon && <span className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-border bg-secondary text-muted-foreground [&_svg]:size-3.5">{icon}</span>}
      <div className="min-w-0 flex-1">
        <h3 className="text-[13px] font-semibold leading-5 tracking-tight">{title}</h3>
        {description && <p className="text-[12px] leading-4 text-muted-foreground">{description}</p>}
      </div>
      {right && <div className="flex shrink-0 items-center gap-1.5">{right}</div>}
    </div>
  );
}

export function CardBody({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("px-5 py-4", className)} {...props} />;
}

/** Settings-style row: label + description on the left, control on the right. */
export function Row({ label, description, children, className }: { label: string; description?: string; children: React.ReactNode; className?: string }) {
  return (
    <div className={cn("flex items-center justify-between gap-6 py-3 first:pt-0 last:pb-0 [&+&]:border-t [&+&]:border-border", className)}>
      <div className="min-w-0">
        <div className="text-[13px] font-medium">{label}</div>
        {description && <div className="mt-0.5 text-[12px] leading-4 text-muted-foreground">{description}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

export function PageHeader({ title, description, right }: { title: string; description?: string; right?: React.ReactNode }) {
  return (
    <div className="mb-5 flex items-end justify-between gap-4">
      <div>
        <h1 className="text-[18px] font-semibold tracking-tight">{title}</h1>
        {description && <p className="mt-0.5 text-[12.5px] text-muted-foreground">{description}</p>}
      </div>
      {right && <div className="flex items-center gap-2">{right}</div>}
    </div>
  );
}
