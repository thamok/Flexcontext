import React, { useState } from 'react';
export const useSelection = (initial) => {
  const [selected, setSelected] = useState(initial);
  const toggle = (id) => {
    setSelected(current => current.includes(id) ? current.filter(value => value !== id) : [...current, id]);
  };
  return { selected, toggle };
};
export const SelectionList = React.memo(({ items }) => {
  const { selected, toggle } = useSelection([]);
  return <ul>{items.map(item => <li key={item.id} onClick={() => toggle(item.id)} aria-selected={selected.includes(item.id)}>{item.label}</li>)}</ul>;
});
export const selectionHeading = () => 'Selection list selected items toggle';
