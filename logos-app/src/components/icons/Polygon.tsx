import React from "react";
import type { SVGProps } from "react";

export interface PolygonProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const Polygon = React.forwardRef<SVGSVGElement, PolygonProps>(
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
        <path fill="currentColor" d="M52.3 308.1L45.5 320L52.3 331.9L176 548L182.9 560.1L458 560.1L464.9 548L588.6 331.9L595.4 320L588.6 308.1L464.9 92L458 79.9L182.9 79.9L176 92L52.3 308.1z"/>
      </svg>
    );
  }
);

Polygon.displayName = "Polygon";

export default Polygon;
