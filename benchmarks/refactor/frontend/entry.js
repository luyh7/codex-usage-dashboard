import { mount, tick } from 'svelte';
import Table from './Table.svelte';

window.mountSvelteProbe = () => {
  const target = document.getElementById('sessionRows');
  target.replaceChildren();
  const component = mount(Table, { target });
  return async blocks => { component.update(blocks); await tick(); };
};
