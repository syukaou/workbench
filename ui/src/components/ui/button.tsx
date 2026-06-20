import { forwardRef } from 'react';
import { cn } from '../../lib/utils';

/**
 * shadcn-style Button — the surrounding-tool layer (DESIGN §2).
 * Behaviour/markup follow shadcn's pattern; styling reads ONLY the single
 * design-token source via the `.ui-button*` classes in App.css (no second
 * palette, no Tailwind). Variants/sizes map to CSS classes.
 */
export type ButtonVariant = 'default' | 'primary' | 'ghost' | 'success' | 'destructive';
export type ButtonSize = 'sm' | 'md' | 'icon';

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Renders the pressed/active styling (e.g. a toggled toolbar mode). */
  active?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = 'default', size = 'md', active = false, type = 'button', ...props }, ref) => (
    <button
      ref={ref}
      type={type}
      data-active={active || undefined}
      className={cn(
        'ui-button',
        `ui-button--${variant}`,
        `ui-button--${size}`,
        className,
      )}
      {...props}
    />
  ),
);
Button.displayName = 'Button';
