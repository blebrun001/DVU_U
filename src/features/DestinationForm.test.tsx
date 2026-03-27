import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DestinationForm } from './DestinationForm';

vi.mock('../lib/api', () => ({
  saveDestination: vi.fn(),
  testDestination: vi.fn()
}));

import * as api from '../lib/api';

const mockedApi = vi.mocked(api);

describe('DestinationForm', () => {
  beforeEach(() => {
    vi.useRealTimers();
    mockedApi.saveDestination.mockResolvedValue({ ok: true });
    mockedApi.testDestination.mockResolvedValue({
      ok: true,
      datasetTitle: 'Test dataset'
    });
  });

  it('disables controls when form is locked', () => {
    render(<DestinationForm disabled />);

    expect(screen.getByPlaceholderText('https://demo.dataverse.org')).toBeDisabled();
    expect(screen.getByPlaceholderText('doi:10.xxxx/XXXX')).toBeDisabled();
    expect(screen.getByPlaceholderText('********')).toBeDisabled();
  });

  it('shows success state and saves destination after validation', async () => {
    const { container } = render(<DestinationForm />);

    fireEvent.change(screen.getByPlaceholderText('https://demo.dataverse.org'), {
      target: { value: 'https://demo.dataverse.org' }
    });
    fireEvent.change(screen.getByPlaceholderText('doi:10.xxxx/XXXX'), {
      target: { value: 'doi:10.1234/ABC' }
    });
    fireEvent.change(screen.getByPlaceholderText('********'), {
      target: { value: 'my-token' }
    });

    await waitFor(() => {
      expect(mockedApi.testDestination).toHaveBeenCalledOnce();
      expect(mockedApi.saveDestination).toHaveBeenCalledOnce();
    });
    expect(container.querySelectorAll('.input-status-icon.ok')).toHaveLength(3);
    expect(screen.getByText('Dataset name : Test dataset')).toBeInTheDocument();
  });

  it('marks dataset field as invalid when API returns dataset_not_found', async () => {
    mockedApi.testDestination.mockResolvedValueOnce({
      ok: false,
      errorKind: 'dataset_not_found',
      message: 'Dataset not found'
    });
    const { container } = render(<DestinationForm />);

    fireEvent.change(screen.getByPlaceholderText('https://demo.dataverse.org'), {
      target: { value: 'https://demo.dataverse.org' }
    });
    fireEvent.change(screen.getByPlaceholderText('doi:10.xxxx/XXXX'), {
      target: { value: 'doi:10.1234/MISSING' }
    });
    fireEvent.change(screen.getByPlaceholderText('********'), {
      target: { value: 'my-token' }
    });

    await waitFor(() => {
      expect(mockedApi.testDestination).toHaveBeenCalledOnce();
    });

    expect(container.querySelectorAll('.input-status-icon.error')).toHaveLength(1);
    expect(container.querySelectorAll('.input-status-icon.ok')).toHaveLength(2);
    expect(mockedApi.saveDestination).not.toHaveBeenCalled();
  });
});
