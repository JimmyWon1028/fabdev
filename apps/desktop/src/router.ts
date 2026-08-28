import { createRouter, createWebHashHistory } from 'vue-router'

import DashboardView from './views/DashboardView.vue'
import MariaDbView from './views/MariaDbView.vue'
import NodeJsView from './views/NodeJsView.vue'
import ProxyView from './views/ProxyView.vue'
import RuntimesView from './views/RuntimesView.vue'
import SettingsView from './views/SettingsView.vue'
import SitesView from './views/SitesView.vue'

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', component: DashboardView },
    { path: '/mariadb', component: MariaDbView },
    { path: '/nodejs', component: NodeJsView },
    { path: '/proxy', component: ProxyView },
    { path: '/sites', component: SitesView },
    { path: '/runtimes', component: RuntimesView },
    { path: '/settings', component: SettingsView }
  ]
})
