import { zodResolver } from '@hookform/resolvers/zod';
import { useEffect, useRef, useState } from 'react';
import { useForm } from 'react-hook-form';
import { z } from 'zod';
import { saveDestination, testDestination } from '../lib/api';
import type { DestinationBootstrap, DestinationValidationResult } from '../lib/types';

const schema = z.object({
  serverUrl: z.string().url('Enter a valid URL'),
  datasetPid: z.string().min(1, 'Dataset PID is required'),
  apiToken: z.string().optional().default('')
});

type FormData = z.infer<typeof schema>;
type FieldStatus = 'idle' | 'ok' | 'error';

interface DestinationFormProps {
  initialDestination?: DestinationBootstrap | null;
  disabled?: boolean;
  resetSignal?: number;
}

export function DestinationForm({
  initialDestination,
  disabled = false,
  resetSignal = 0
}: DestinationFormProps) {
  const initialValues = {
    serverUrl: initialDestination?.serverUrl ?? '',
    datasetPid: initialDestination?.datasetPid ?? '',
    apiToken: ''
  };
  const {
    register,
    watch,
    reset,
    formState: { errors, isSubmitting }
  } = useForm<FormData>({
    resolver: zodResolver(schema),
    defaultValues: initialValues
  });

  const [validation, setValidation] =
    useState<DestinationValidationResult | null>(null);
  const [serverStatus, setServerStatus] = useState<FieldStatus>('idle');
  const [datasetStatus, setDatasetStatus] = useState<FieldStatus>('idle');
  const [tokenStatus, setTokenStatus] = useState<FieldStatus>('idle');
  const lastSavedSignature = useRef<string>('');
  const hasHandledResetSignal = useRef(false);
  const controlsDisabled = disabled || isSubmitting;
  const serverUrlValue = watch('serverUrl');
  const datasetPidValue = watch('datasetPid');
  const apiTokenValue = watch('apiToken');

  useEffect(() => {
    reset(initialValues);
  }, [initialDestination?.serverUrl, initialDestination?.datasetPid, reset]);

  useEffect(() => {
    if (!hasHandledResetSignal.current) {
      hasHandledResetSignal.current = true;
      return;
    }
    reset({
      serverUrl: '',
      datasetPid: '',
      apiToken: ''
    });
    setValidation(null);
    setServerStatus('idle');
    setDatasetStatus('idle');
    setTokenStatus('idle');
    lastSavedSignature.current = '';
  }, [resetSignal, reset]);

  useEffect(() => {
    const serverUrl = (serverUrlValue ?? '').trim();
    const datasetPid = (datasetPidValue ?? '').trim();
    const apiToken = (apiTokenValue ?? '').trim();

    if (!serverUrl || !datasetPid || !apiToken) {
      setServerStatus('idle');
      setDatasetStatus('idle');
      setTokenStatus('idle');
      return;
    }

    let active = true;
    const timer = window.setTimeout(async () => {
      try {
        const result = await testDestination({
          serverUrl,
          datasetPid,
          apiToken
        });
        if (!active) {
          return;
        }
        setValidation(result);
        if (result.ok) {
          setServerStatus('ok');
          setDatasetStatus('ok');
          setTokenStatus('ok');
          const signature = `${serverUrl}::${datasetPid}::${apiToken}`;
          if (lastSavedSignature.current !== signature) {
            await saveDestination({
              serverUrl,
              datasetPid,
              apiToken
            });
            lastSavedSignature.current = signature;
          }
          return;
        }

        const kind = result.errorKind;
        if (kind === 'dataset_not_found') {
          setServerStatus('ok');
          setDatasetStatus('error');
          setTokenStatus('ok');
          return;
        }
        if (kind === 'auth' || kind === 'permission') {
          setServerStatus('ok');
          setDatasetStatus('ok');
          setTokenStatus('error');
          return;
        }
        if (kind === 'invalid_input') {
          if (result.message?.toLowerCase().includes('url')) {
            setServerStatus('error');
            setDatasetStatus('ok');
            setTokenStatus('ok');
            return;
          }
          setServerStatus('ok');
          setDatasetStatus('error');
          setTokenStatus('ok');
          return;
        }

        setServerStatus('error');
        setDatasetStatus('error');
        setTokenStatus('error');
      } catch {
        if (!active) {
          return;
        }
        setServerStatus('error');
        setDatasetStatus('error');
        setTokenStatus('error');
      }
    }, 650);

    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [serverUrlValue, datasetPidValue, apiTokenValue]);

  return (
    <section className="card">
      <header className="card-header">
        <h2>Destination</h2>
        <p>Configure your Dataverse server and target dataset.</p>
      </header>
      <form className="form-grid" onSubmit={(e) => e.preventDefault()}>
        <label>
          Server URL
          <div className="input-status-wrap">
            <input
              placeholder="https://demo.dataverse.org"
              {...register('serverUrl')}
              disabled={controlsDisabled}
            />
            <FieldStatusIcon status={serverStatus} />
          </div>
          {errors.serverUrl && <span className="error-text">{errors.serverUrl.message}</span>}
        </label>
        <label>
          Dataset PID
          <div className="input-status-wrap">
            <input
              placeholder="doi:10.xxxx/XXXX"
              {...register('datasetPid')}
              disabled={controlsDisabled}
            />
            <FieldStatusIcon status={datasetStatus} />
          </div>
          {errors.datasetPid && <span className="error-text">{errors.datasetPid.message}</span>}
        </label>
        <label>
          API Token
          <div className="input-status-wrap">
            <input
              type="password"
              placeholder="********"
              {...register('apiToken')}
              disabled={controlsDisabled}
            />
            <FieldStatusIcon status={tokenStatus} />
          </div>
          {errors.apiToken && <span className="error-text">{errors.apiToken.message}</span>}
        </label>
        {validation?.ok && validation.datasetTitle && (
          <p className="mini-text">Dataset name : {validation.datasetTitle}</p>
        )}
        {initialDestination?.hasToken && (
          <p className="mini-text">
            A token is already stored. Leave this field empty to reuse it.
          </p>
        )}
      </form>
    </section>
  );
}

function FieldStatusIcon({ status }: { status: FieldStatus }) {
  if (status === 'idle') {
    return null;
  }
  return (
    <span className={`input-status-icon ${status === 'ok' ? 'ok' : 'error'}`} aria-hidden="true">
      {status === 'ok' ? '✓' : '✕'}
    </span>
  );
}
