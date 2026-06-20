import { clsx, type ClassValue } from 'clsx';

/**
 * `cn` — the shadcn class-name helper. Plain `clsx` here (no Tailwind merge):
 * WORKBENCH styles components with the single design-token source (DESIGN §2),
 * not Tailwind utilities, so there are no conflicting utility classes to merge.
 */
export function cn(...inputs: ClassValue[]): string {
  return clsx(inputs);
}
