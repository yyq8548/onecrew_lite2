import icon from '@/assets/onecrew-icon.svg';

export function BrandLogo({ className = '' }: { className?: string }) {
  return (
    <span className={`onecrew-lite-logo ${className}`}>
      <img src={icon} alt="" />
      <strong>onecrew_lite</strong>
    </span>
  );
}
