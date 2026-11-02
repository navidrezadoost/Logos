import React from "react";
import type { SVGProps } from "react";

export interface ChevronDownProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const ChevronDown = React.forwardRef<SVGSVGElement, ChevronDownProps>(
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
        <path fill="currentColor" d="M320.3 493.3L534.9 278.7L557.5 256.1L512.2 210.8L489.6 233.4L320.2 402.8L150.8 233.4L128.2 210.8L82.9 256.1L105.5 278.7L297.5 470.7L320.1 493.3z"/>
      </svg>
    );
  }
);

ChevronDown.displayName = "ChevronDown";

export default ChevronDown;
