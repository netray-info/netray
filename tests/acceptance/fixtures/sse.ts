export interface SseEvent {
  event: string;
  data: string;
  id?: string;
}

export async function consumeSSE(
  url: string,
  headers: Record<string, string>,
  timeoutMs: number = 30_000,
): Promise<SseEvent[]> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(url, {
      headers: {
        Accept: 'text/event-stream',
        ...headers,
      },
      signal: controller.signal,
    });

    if (!response.ok) {
      throw new Error(`SSE request failed with status ${response.status}`);
    }

    const text = await response.text();
    return parseSseText(text);
  } catch (err: any) {
    if (err.name === 'AbortError') {
      throw new Error(`SSE timeout after ${timeoutMs}ms for ${url}`);
    }
    throw err;
  } finally {
    clearTimeout(timeout);
  }
}

function parseSseText(text: string): SseEvent[] {
  const events: SseEvent[] = [];
  let currentEvent: Partial<SseEvent> = {};
  let dataLines: string[] = [];

  for (const line of text.split('\n')) {
    if (line === '') {
      // Empty line = event boundary
      if (dataLines.length > 0 || currentEvent.event) {
        events.push({
          event: currentEvent.event ?? 'message',
          data: dataLines.join('\n'),
          ...(currentEvent.id ? { id: currentEvent.id } : {}),
        });
      }
      currentEvent = {};
      dataLines = [];
      continue;
    }

    if (line.startsWith('event:')) {
      currentEvent.event = line.slice(6).trim();
    } else if (line.startsWith('data:')) {
      dataLines.push(line.slice(5).trim());
    } else if (line.startsWith('id:')) {
      currentEvent.id = line.slice(3).trim();
    }
  }

  // Handle last event if no trailing newline
  if (dataLines.length > 0 || currentEvent.event) {
    events.push({
      event: currentEvent.event ?? 'message',
      data: dataLines.join('\n'),
      ...(currentEvent.id ? { id: currentEvent.id } : {}),
    });
  }

  return events;
}
