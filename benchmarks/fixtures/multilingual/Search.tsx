import React from 'react';
export const filterSuggestions = (values: string[], query: string): string[] => {
  return values.filter(value => value.toLowerCase().startsWith(query.toLowerCase()));
};
export const SearchBox = React.forwardRef<HTMLInputElement, { query: string; values: string[] }>((props, ref) => {
  const suggestions = filterSuggestions(props.values, props.query);
  return <div><input ref={ref} value={props.query} /><ul>{suggestions.map(value => <li key={value}>{value}</li>)}</ul></div>;
});
export const searchHeading = () => 'Search box query filter suggestions values';
