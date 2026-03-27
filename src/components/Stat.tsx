interface StatProps {
  label: string;
  value: string | number;
  muted?: boolean;
}

export function Stat({ label, value, muted = false }: StatProps) {
  return (
    <div className={`stat ${muted ? 'muted' : ''}`}>
      <span className="stat-label">{label}</span>
      <strong className="stat-value">{value}</strong>
    </div>
  );
}
