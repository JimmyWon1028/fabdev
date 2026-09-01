import { describe, expect, it } from 'vitest'

import {
  dynamicModuleRegistry,
  resolveDynamicModules,
  type DynamicModuleDefinition
} from './navigation'

describe('dynamic sidebar modules', () => {
  it('shows modules supplied by the Runtime Catalog in registry order', () => {
    const modules = resolveDynamicModules(dynamicModuleRegistry, {
      artifacts: [{ name: 'node' }, { name: 'mariadb' }],
      installedPackageNames: []
    })

    expect(modules.map((module) => module.id)).toEqual(['mariadb', 'nodejs'])
  })

  it('keeps installed modules visible without Catalog artifacts', () => {
    const modules = resolveDynamicModules(dynamicModuleRegistry, {
      artifacts: [],
      installedPackageNames: ['node']
    })

    expect(modules.map((module) => module.id)).toEqual(['nodejs'])
  })

  it('hides the dynamic section when no module is available or installed', () => {
    expect(resolveDynamicModules(dynamicModuleRegistry, {
      artifacts: [],
      installedPackageNames: []
    })).toEqual([])
  })

  it('supports a future capability-backed module without a package', () => {
    const registry: DynamicModuleDefinition[] = [{
      id: 'ai',
      route: '/ai',
      labelKey: 'nav.dashboard',
      order: 30,
      packageNames: [],
      capabilityNames: ['ai']
    }]

    expect(resolveDynamicModules(registry, {
      artifacts: [],
      installedPackageNames: [],
      availableCapabilities: ['ai']
    }).map((module) => module.id)).toEqual(['ai'])
  })
})
