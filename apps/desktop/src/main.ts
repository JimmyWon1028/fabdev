import { createPinia } from 'pinia'
import { createApp } from 'vue'

import App from './App.vue'
import { router } from './router'
import { initializeTheme } from './utils/theme'
import './styles.css'
import './themes/neo-brutalism.css'
import './themes/glassmorphism.css'
import './themes/cyberpunk.css'
import './themes/cyberpunk-city.css'
import './themes/notion.css'

initializeTheme()
createApp(App).use(createPinia()).use(router).mount('#app')
