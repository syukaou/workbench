import * as SeparatorPrimitive from '@radix-ui/react-separator';
import { forwardRef } from 'react';
import { cn } from '../../lib/utils';

/**
 * shadcn-style Separator on the Radix primitive (DESIGN §1 "Radix 打底").
 * Reads the single design-token source via `.ui-separator` in App.css.
 */
export const Separator = forwardRef<
  React.ComponentRef<typeof SeparatorPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof SeparatorPrimitive.Root>
>(({ className, orientation = 'vertical', decorative = true, ...props }, ref) => (
  <SeparatorPrimitive.Root
    ref={ref}
    decorative={decorative}
    orientation={orientation}
    className={cn('ui-separator', `ui-separator--${orientation}`, className)}
    {...props}
  />
));
Separator.displayName = SeparatorPrimitive.Root.displayName;
