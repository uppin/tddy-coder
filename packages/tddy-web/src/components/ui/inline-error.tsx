import { cn } from "@/lib/utils"

/**
 * A failure shown where it happened — beside the form, card or list whose action or load failed.
 * Renders nothing while there is no `message`. `role="alert"` so assistive tech announces it.
 */
function InlineError({
  message,
  testId,
  className,
}: {
  message: string | null | undefined
  testId: string
  className?: string
}) {
  if (!message) return null
  return (
    <p role="alert" data-testid={testId} className={cn("text-sm text-destructive", className)}>
      {message}
    </p>
  )
}

export { InlineError }
