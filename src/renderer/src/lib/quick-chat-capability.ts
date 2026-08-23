import { isWebClientLocation } from '@/lib/web-client-location'

export function isQuickChatCreationAvailable(): boolean {
  return !isWebClientLocation()
}
