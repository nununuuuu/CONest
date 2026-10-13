/** UI routing only. Platform parsing stays in each provider's adapter. */
export type ProviderId = 'bilibili' | 'ytdlp';

export function chooseProvider(
  input: string,
  explicitBilibiliType = false,
): ProviderId {
  if (
    explicitBilibiliType ||
    /^(av\d+|BV\w{10}|ep\d+|ss\d+|md\d+|au\d+|am\d+|cv\d+)$/i.test(input.trim())
  )
    return 'bilibili';
  // Shared browser text and missing scheme must retain the original parser's
  // behavior, including Bilibili URLs embedded in a share message.
  if (/(?:https?:\/\/)?(?:[\w-]+\.)*(?:bilibili\.com|b23\.tv)\//i.test(input))
    return 'bilibili';
  return 'ytdlp';
}
