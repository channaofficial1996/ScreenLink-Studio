import icon from "../assets/app-icon.png";

export function Logo({ size = 34 }: { size?: number }) {
  return <img src={icon} width={size} height={size} alt="" className="logo" draggable={false} />;
}
