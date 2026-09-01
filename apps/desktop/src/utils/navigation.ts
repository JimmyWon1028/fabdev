import type { RuntimeUpdateArtifact } from '@fabdev/contracts'

import type { TranslationKey } from './locales'

export interface DynamicModuleDefinition {
  id: string
  route: string
  labelKey: TranslationKey
  order: number
  packageNames: readonly string[]
  capabilityNames?: readonly string[]
}

export interface DynamicModuleAvailability {
  artifacts: ReadonlyArray<Pick<RuntimeUpdateArtifact, 'name'>>
  installedPackageNames: Iterable<string>
  availableCapabilities?: Iterable<string>
}

export const dynamicModuleRegistry = [
  {
    id: 'mariadb',
    route: '/mariadb',
    labelKey: 'nav.mariadb',
    order: 10,
    packageNames: ['mariadb']
  },
  {
    id: 'nodejs',
    route: '/nodejs',
    labelKey: 'nav.nodejs',
    order: 20,
    packageNames: ['node']
  }
] as const satisfies readonly DynamicModuleDefinition[]

export function resolveDynamicModules(
  registry: readonly DynamicModuleDefinition[],
  availability: DynamicModuleAvailability
): DynamicModuleDefinition[] {
  const availablePackageNames = new Set(availability.artifacts.map((artifact) => artifact.name))
  const installedPackageNames = new Set(availability.installedPackageNames)
  const availableCapabilities = new Set(availability.availableCapabilities ?? [])

  return registry
    .filter((module) =>
      module.packageNames.some((name) =>
        availablePackageNames.has(name) || installedPackageNames.has(name)
      ) || module.capabilityNames?.some((name) => availableCapabilities.has(name))
    )
    .sort((left, right) => left.order - right.order)
}
