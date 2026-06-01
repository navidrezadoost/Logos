import React from "react";
import type { SVGProps } from "react";

export interface PrototypeProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const Prototype = React.forwardRef<SVGSVGElement, PrototypeProps>(
  ({ size, className, style, ...props }, ref) => {
    const dimensions = size ? { width: size, height: size } : {
      width: props.width || 24,
      height: props.height || 24
    };

    return (
      <svg
        ref={ref}
        viewBox="0 0 640 640"
        xmlns="http://www.w3.org/2000/svg"
        width={dimensions.width}
        height={dimensions.height}
        fill={props.fill || "currentColor"}
        className={className}
        style={style}
        {...props}
      >
        <path fill="currentColor" d="M64 96L256 96L256 160L384 160L384 96L576 96L576 288L384 288L384 224L256 224L256 266.7L320 352L448 352L448 544L256 544L256 373.3L192 288L64 288L64 96z"/>
      </svg>
    );
  }
);

Prototype.displayName = "Prototype";

export default Prototype;
