import type { Site } from '@fabdev/contracts'
import { createPinia, setActivePinia } from 'pinia'
import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { afterEach, describe, expect, it } from 'vitest'

import { useAppStore } from '../stores/fabdev'
import { setLanguage, translate } from '../utils/i18n'
import SitesView from './SitesView.vue'

const site: Site = {
  id: 'demo',
  name: 'Demo',
  domain: 'demo.test',
  projectPath: '/Users/dev/Sites/demo',
  documentRoot: '/Users/dev/Sites/demo/public',
  phpVersion: '8.2',
  enabled: true,
  secured: false,
  upstreamResponseTimeoutSeconds: 120
}

afterEach(() => setLanguage('en'))

describe('Sites without LAN sharing', () => {
  it('renders existing Site controls without sharing controls in every language', async () => {
    for (const language of ['en', 'zh-TW', 'zh-CN'] as const) {
      setLanguage(language)
      const pinia = createPinia()
      setActivePinia(pinia)
      const store = useAppStore()
      store.sites = [site, { ...site, id: 'secure', domain: 'secure.test', secured: true }]
      const html = await renderToString(createSSRApp(SitesView).use(pinia))

      expect(html).toContain('demo.test')
      expect(html).toContain('secure.test')
      expect(html).toContain('>HTTPS</span>')
      expect(html).toContain('site-php-select')
      expect(html).toContain(translate('sites.filterHome'))
      expect(html).toContain(translate('sites.filterLinked'))
      expect(html).not.toContain('Web Root')
      expect(html).not.toContain(site.documentRoot)
      expect(html).not.toMatch(/fabDev Connect|Share to LAN|Not shared|>Shared|局網分享|未分享|局网共享|lanShare|value="shared"|is-shared|site-share-address/)
    }
  })

  it('renders an empty Site list without sharing status or filters', async () => {
    const pinia = createPinia()
    const html = await renderToString(createSSRApp(SitesView).use(pinia))

    expect(html).not.toContain('class="site-row"')
    expect(html).not.toContain('value="shared"')
    expect(html).not.toContain('fabDev Connect')
  })
})
