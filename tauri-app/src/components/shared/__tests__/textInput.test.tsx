import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { TextInput } from '../TextInput';

afterEach(cleanup);

describe('TextInput deferred names', () => {
  it('keeps typing intact while the saved value is unchanged and commits once on blur', () => {
    const save = vi.fn();
    const { rerender } = render(<TextInput label="Name" value="Rectangle" onChange={save} commitOnBlur />);
    const input = screen.getByRole('textbox');
    fireEvent.change(input, { target: { value: 'P' } });
    rerender(<TextInput label="Name" value="Rectangle" onChange={save} commitOnBlur />);
    expect(input).toHaveProperty('value', 'P');
    fireEvent.change(input, { target: { value: 'Panel' } });
    expect(save).not.toHaveBeenCalled();
    fireEvent.blur(input);
    expect(save).toHaveBeenCalledExactlyOnceWith('Panel');
    rerender(<TextInput label="Name" value="Panel" onChange={save} commitOnBlur />);
    expect(input).toHaveProperty('value', 'Panel');
  });

  it('commits on Enter without interrupting IME composition', () => {
    const save = vi.fn();
    render(<TextInput label="Name" value="Rectangle" onChange={save} commitOnBlur />);
    const input = screen.getByRole('textbox');
    input.focus();
    fireEvent.change(input, { target: { value: 'Panel' } });
    fireEvent.keyDown(input, { key: 'Enter', isComposing: true });
    expect(save).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(save).toHaveBeenCalledExactlyOnceWith('Panel');
  });

  it('does not carry a draft into a different selected object', () => {
    const save = vi.fn();
    const { rerender } = render(<TextInput key="one" label="Name" value="Rectangle" onChange={save} commitOnBlur />);
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'Draft' } });
    rerender(<TextInput key="two" label="Name" value="Circle" onChange={save} commitOnBlur />);
    expect(screen.getByRole('textbox')).toHaveProperty('value', 'Circle');
    fireEvent.blur(screen.getByRole('textbox'));
    expect(save).not.toHaveBeenCalled();
  });

  it('preserves immediate updates for ordinary controlled fields', () => {
    const change = vi.fn();
    render(<TextInput label="Name" value="Rectangle" onChange={change} />);
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'Panel' } });
    expect(change).toHaveBeenCalledExactlyOnceWith('Panel');
  });
});
