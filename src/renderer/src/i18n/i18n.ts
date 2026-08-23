import i18next, {
  type BackendModule,
  type i18n as I18nInstance,
  type ReadCallback,
  type TOptions
} from 'i18next'
import { initReactI18next } from 'react-i18next'

import { isPseudoLocalizationLocale, pseudoLocalizeString } from './pseudo-localization'
import { DEFAULT_LOCALE, resolveUiLocale } from './supported-languages'
import type { SupportedUiLocale } from '../../../shared/ui-locale'
import { isPluginUiLanguage, type UiLanguage } from '../../../shared/ui-language'
import type { PluginLanguagePackRegistration } from '../../../shared/plugins/plugin-language-pack-artifact'

export const i18n: I18nInstance = i18next.createInstance()

// Why: only the English catalog is bundled eagerly. The other four locales add
// ~2MB to the renderer's startup chunk (parsed on every launch) even though the
// app always boots in English and only switches after the persisted UI language
// loads. A lazy backend fetches each non-English catalog on demand, so any
// changeLanguage() call (UI switch or test) transparently loads its bundle
// instead of paying the parse cost at cold start.
// Every locale, English included, loads through the lazy backend: boot renders
// from inline fallbacks while the catalog chunk streams in off the critical path.
const LOCALE_LOADERS: Record<
  SupportedUiLocale,
  () => Promise<{ default: Record<string, unknown> }>
> = {
  en: () => import('./locales/en.json'),
  es: () => import('./locales/es.json'),
  ja: () => import('./locales/ja.json'),
  ko: () => import('./locales/ko.json'),
  zh: () => import('./locales/zh.json')
}

const lazyLocaleBackend: BackendModule = {
  type: 'backend',
  init: () => {},
  read: (language: string, _namespace: string, callback: ReadCallback) => {
    const loader = LOCALE_LOADERS[language as SupportedUiLocale]
    if (!loader) {
      // Unknown locales (plugin packs register their own resources) have
      // nothing for this backend to load.
      callback(null, false)
      return
    }
    loader().then(
      (mod) => callback(null, mod.default),
      (error) => callback(error instanceof Error ? error : new Error(String(error)), false)
    )
  }
}

void i18n
  .use(lazyLocaleBackend)
  .use(initReactI18next)
  .init({
    fallbackLng: DEFAULT_LOCALE,
    lng: DEFAULT_LOCALE,
    // Why: no bundled resources — every catalog (English included) arrives via
    // the lazy backend, so boot never parses a 13k-entry JSON on the critical
    // path; inline fallbacks cover the brief load window.
    partialBundledLanguages: true,
    interpolation: {
      escapeValue: false
    },
    react: {
      useSuspense: false
    }
  })

export function translate(key: string, fallback: string, options?: TOptions): string {
  const value = i18n.t(key, { defaultValue: fallback, ...options })
  return isPseudoLocalizationLocale(i18n.language) ? pseudoLocalizeString(value) : value
}

export async function setRendererUiLanguage(language: UiLanguage): Promise<void> {
  const resolved = resolveUiLocale(language)
  const resourceLanguage = resolveRendererResourceLanguage(resolved)
  const locale =
    isPluginUiLanguage(language) && resourceLanguage === resolved
      ? DEFAULT_LOCALE
      : resourceLanguage
  if (i18n.language !== locale) {
    // changeLanguage triggers the lazy backend load for non-English locales and
    // resolves once the catalog is in memory.
    await i18n.changeLanguage(locale)
  }
}

const registeredPluginLanguages = new Set<string>()
let pluginLanguagePacks: readonly PluginLanguagePackRegistration[] = []

/**
 * BCP-47 tag for `Intl` formatting. Plugin catalogs register under a synthetic
 * `plugin<hex>` resource language that `Intl` rejects, so fall back to the tag
 * the pack declares (for example `ru-RU`) and finally to the default locale.
 */
export function getIntlLocale(): string {
  const active = i18n.language
  const pack = pluginLanguagePacks.find((entry) => entry.resourceLanguage === active)
  const candidate = pack?.locale ?? active
  try {
    // Why: an empty result means Intl has no data for the tag, so fall through
    // to the default locale instead of letting Intl pick the runtime one.
    return Intl.DateTimeFormat.supportedLocalesOf(candidate)[0] ?? DEFAULT_LOCALE
  } catch {
    return DEFAULT_LOCALE
  }
}

export function resolveRendererResourceLanguage(language: string): string {
  return pluginLanguagePacks.find((pack) => pack.id === language)?.resourceLanguage ?? language
}

export function setRendererPluginLanguagePacks(
  packs: readonly PluginLanguagePackRegistration[]
): void {
  for (const language of registeredPluginLanguages) {
    i18n.removeResourceBundle(language, 'translation')
  }
  registeredPluginLanguages.clear()
  pluginLanguagePacks = packs
  for (const pack of packs) {
    i18n.addResourceBundle(pack.resourceLanguage, 'translation', pack.catalog, true, true)
    registeredPluginLanguages.add(pack.resourceLanguage)
  }
}
