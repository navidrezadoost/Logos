import React from "react";
import type { SVGProps } from "react";

export interface EllipseProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const Ellipse = React.forwardRef<SVGSVGElement, EllipseProps>(
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
        <path fill="currentColor" d="M64 320C64 178.6 178.6 64 320 64C461.4 64 576 178.6 576 320C576 461.4 461.4 576 320 576C178.6 576 64 461.4 64 320z"/>
      </svg>
    );
  }
);

Ellipse.displayName = "Ellipse";

export default Ellipse;
