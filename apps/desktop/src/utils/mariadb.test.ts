import { describe, expect, it } from 'vitest'

import { EMPTY_MARIADB_CONFIG, ERP_MARIADB_CONFIG } from './mariadb'

describe('MariaDB configuration presets', () => {
  it('keeps a new installation empty', () => {
    expect(EMPTY_MARIADB_CONFIG).toBe('[mariadbd]\n\n')
  })

  it('provides the validated ERP preset without fabDev-managed options', () => {
    expect(ERP_MARIADB_CONFIG).toContain('character-set-server = utf8')
    expect(ERP_MARIADB_CONFIG).toContain('collation-server = utf8_unicode_ci')
    expect(ERP_MARIADB_CONFIG).toContain('innodb_buffer_pool_size = 4G')
    expect(ERP_MARIADB_CONFIG).toContain('query_cache_size = 0')
    expect(ERP_MARIADB_CONFIG).not.toContain('datadir')
    expect(ERP_MARIADB_CONFIG).not.toContain('port =')
  })
})
