import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { installPressFlash } from './lib/press';

installPressFlash();

const app = mount(App, { target: document.getElementById('app')! });

export default app;
