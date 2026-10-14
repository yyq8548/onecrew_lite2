
import * as React from "react"
import * as ProgressPrimitive from "@radix-ui/react-progress"

import { cn } from "@/lib/utils"

function Progress({
  className,
  value,
  ...props
}: React.ComponentProps<typeof ProgressPrimitive.Root>) {
  return (
    <ProgressPrimitive.Root
      data-slot="progress"
      className={cn(
        "relative h-2 w-full overflow-hidden rounded-full bg-primary/20",
        className
      )}
      {...props}
    >
      {/* value == null 是 Radix 自带的 indeterminate 态;上游 shadcn 模板只
          渲染确定值,扫动条视觉是本仓库补的(styles/index.css)。 */}
      <ProgressPrimitive.Indicator
        data-slot="progress-indicator"
        className={cn(
          "h-full flex-1 bg-primary",
          value == null ? "progress-indeterminate w-1/3" : "w-full transition-all"
        )}
        style={value == null ? undefined : { transform: `translateX(-${100 - (value || 0)}%)` }}
      />
    </ProgressPrimitive.Root>
  )
}

export { Progress }
